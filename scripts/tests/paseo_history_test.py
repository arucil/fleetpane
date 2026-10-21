"""Real local Git DAGs exercise the history reader; no network or source checkout writes."""

import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "paseo-history.py"
SPEC = importlib.util.spec_from_file_location("paseo_history", SCRIPT)
history = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(history)


class HistoryTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="fleetpane-history-test-")
        self.addCleanup(self.temporary.cleanup)
        self.repo = Path(self.temporary.name)
        self.run_git("init", "--initial-branch=main")
        self.run_git("config", "user.name", "Synthetic Fixture")
        self.run_git("config", "user.email", "fixture@example.invalid")
        self.base = self.commit("root.txt", "Initial reference")
        self.shared = self.commit(
            "packages/app/src/timeline/界\n面.ts", "Shared timeline change"
        )
        self.run_git("branch", "fork")
        self.upstream = self.commit(
            "packages/app/src/file-pane/view.ts", "Upstream file change"
        )
        self.run_git("checkout", "fork")
        self.fork = self.commit(
            "packages/server/src/worktree.ts", "Fork worktree change"
        )
        self.run_git("update-ref", "refs/remotes/upstream/main", self.upstream)
        self.run_git("update-ref", "refs/remotes/origin/main", self.fork)
        self.checkpoint = {
            "version": 1,
            "reference_revision": self.base,
            "sources": [
                {
                    "name": "upstream",
                    "ref": "refs/remotes/upstream/main",
                    "revision": self.shared,
                    "url": "https://github.com/getpaseo/paseo",
                },
                {
                    "name": "fork",
                    "ref": "refs/remotes/origin/main",
                    "revision": self.shared,
                    "url": "https://github.com/DonutLabs-ai/paseo",
                },
            ],
        }

    def run_git(self, *args):
        return (
            subprocess.check_output(
                ["git", "-c", "commit.gpgsign=false", "-C", str(self.repo), *args],
                stderr=subprocess.PIPE,
            )
            .decode()
            .strip()
        )

    def commit(self, path, message):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(message)
        self.run_git("add", "--", path)
        self.run_git("commit", "-m", message)
        return self.run_git("rev-parse", "HEAD")

    def test_reference_deduplicates_shared_commits_and_preserves_paths(self):
        # The original fork reference need not be an ancestor of upstream.
        report = history.collect(self.repo, self.checkpoint, from_reference=True)
        self.assertEqual(report["unique_commits"], 3)
        self.assertEqual([row["new_commits"] for row in report["sources"]], [2, 2])
        shared = next(
            row for row in report["commits"] if row["revision"] == self.shared
        )
        self.assertEqual(shared["sources"], ["upstream", "fork"])
        self.assertEqual(shared["paths"], ["packages/app/src/timeline/界\n面.ts"])
        self.assertEqual(shared["topics"], ["streaming-and-navigation"])
        self.assertEqual(len(shared["links"]), 2)

    def test_incremental_is_per_source_and_ignores_dirty_content(self):
        target = self.repo / "root.txt"
        target.write_text("Uncommitted user work")
        (self.repo / "untracked.txt").write_text("User notes")
        before = (self.repo / ".git/index").read_bytes()
        report = history.collect(self.repo, self.checkpoint)
        self.assertEqual(report["unique_commits"], 2)
        self.assertTrue(report["worktree_dirty"])
        self.assertNotIn("Uncommitted user work", json.dumps(report))
        self.assertEqual(target.read_text(), "Uncommitted user work")
        self.assertEqual(before, (self.repo / ".git/index").read_bytes())
        self.assertEqual(self.run_git("rev-parse", "HEAD"), self.fork)

    def test_checkpoint_delta_can_be_empty(self):
        self.checkpoint["sources"][0]["revision"] = self.upstream
        self.checkpoint["sources"][1]["revision"] = self.fork
        report = history.collect(self.repo, self.checkpoint)
        self.assertEqual(report["commits"], [])
        self.assertEqual(report["unique_commits"], 0)

    def test_from_root_includes_initial_commit_and_orders_both_lineages(self):
        # Saved delta anchors are deliberately unavailable: root mode must not
        # subtract them or rely on the September reference's existence.
        self.checkpoint["reference_revision"] = "f" * 40
        for source in self.checkpoint["sources"]:
            source["revision"] = "e" * 40
        report = history.collect(self.repo, self.checkpoint, from_root=True)
        self.assertEqual(report["mode"], "from-root")
        self.assertEqual(report["unique_commits"], 4)
        self.assertEqual(report["commits"][0]["revision"], self.base)
        self.assertEqual(report["commits"][0]["parents"], [])
        self.assertEqual(report["commits"][0]["sources"], ["upstream", "fork"])
        self.assertEqual([s["commits_in_scope"] for s in report["sources"]], [3, 3])
        self.assertTrue(
            all(
                s["baseline"] is None and s["roots"] == [self.base]
                for s in report["sources"]
            )
        )
        seen = set()
        for ordinal, commit in enumerate(report["commits"], 1):
            self.assertEqual(commit["ordinal"], ordinal)
            self.assertTrue(set(commit["parents"]) <= seen)
            seen.add(commit["revision"])
        self.assertEqual(report["commits"][0]["paths"], ["root.txt"])

    def test_root_scope_is_exclusive_and_still_enforces_limits(self):
        with self.assertRaisesRegex(history.HistoryError, "not both"):
            history.collect(
                self.repo, self.checkpoint, from_reference=True, from_root=True
            )
        with self.assertRaisesRegex(history.HistoryError, "no partial report"):
            history.collect(self.repo, self.checkpoint, from_root=True, limit=2)

    def test_root_review_batches_preserve_totals_and_advance_exact_cursor(self):
        report = history.collect(self.repo, self.checkpoint, from_root=True)
        first = history.batch(report, take=2)
        self.assertEqual(first["unique_commits"], 4)
        self.assertEqual(first["selection"]["returned"], 2)
        self.assertTrue(first["selection"]["has_more"])
        self.assertEqual(first["commits"][-1]["revision"], self.shared)
        second = history.batch(report, first["selection"]["next_after"], 2)
        self.assertFalse(second["selection"]["has_more"])
        self.assertEqual([c["ordinal"] for c in second["commits"]], [3, 4])
        self.assertEqual(len(report["commits"]), 4)
        self.assertEqual(
            history.batch(report, second["selection"]["next_after"], 2)["commits"], []
        )
        with self.assertRaisesRegex(history.HistoryError, "full commit SHA"):
            history.batch(report, "f" * 40, 2)
        with self.assertRaisesRegex(history.HistoryError, "full commit SHA"):
            history.batch(report, self.base[:9], 2)
        with self.assertRaisesRegex(history.HistoryError, "between 1"):
            history.batch(report, take=0)
        with self.assertRaisesRegex(history.HistoryError, "require --from-root"):
            history.batch(history.collect(self.repo, self.checkpoint), take=2)

    def test_root_traversal_preserves_parents_through_merge_and_skewed_dates(self):
        self.run_git("merge", "--no-ff", "main", "-m", "Merge upstream")
        subprocess.run(
            [
                "git",
                "-c",
                "commit.gpgsign=false",
                "-C",
                str(self.repo),
                "commit",
                "--amend",
                "--no-edit",
            ],
            env={**os.environ, "GIT_COMMITTER_DATE": "2001-01-01T00:00:00+00:00"},
            capture_output=True,
            check=True,
        )
        self.run_git(
            "update-ref", "refs/remotes/origin/main", self.run_git("rev-parse", "HEAD")
        )
        report = history.collect(self.repo, self.checkpoint, from_root=True)
        seen = set()
        for commit in report["commits"]:
            self.assertTrue(set(commit["parents"]) <= seen)
            seen.add(commit["revision"])
        self.assertTrue(report["commits"][-1]["merge"])
        self.assertTrue(report["commits"][-1]["date"].startswith("2001-01-01"))

    def test_rewritten_or_missing_ref_refuses_incremental_report(self):
        self.run_git("update-ref", "refs/remotes/upstream/main", self.base)
        with self.assertRaisesRegex(history.HistoryError, "not an ancestor"):
            history.collect(self.repo, self.checkpoint)
        self.run_git("update-ref", "-d", "refs/remotes/upstream/main")
        with self.assertRaises(history.HistoryError):
            history.collect(self.repo, self.checkpoint)

    def test_limits_do_not_silently_omit_history(self):
        with self.assertRaisesRegex(history.HistoryError, "no partial report"):
            history.collect(self.repo, self.checkpoint, from_reference=True, limit=1)

    def test_shallow_clone_is_rejected_before_comparing_refs(self):
        clone = self.repo / "shallow"
        subprocess.run(
            ["git", "clone", "--depth=1", self.repo.as_uri(), str(clone)],
            capture_output=True,
            check=True,
        )
        with self.assertRaisesRegex(history.HistoryError, "Full Git history required"):
            history.collect(clone, self.checkpoint)

    def test_merge_is_identified_without_inventing_changed_paths(self):
        self.run_git("merge", "--no-ff", "main", "-m", "Merge upstream")
        merged = self.run_git("rev-parse", "HEAD")
        self.run_git("update-ref", "refs/remotes/origin/main", merged)
        report = history.collect(self.repo, self.checkpoint)
        row = next(row for row in report["commits"] if row["revision"] == merged)
        self.assertTrue(row["merge"])
        self.assertEqual(row["paths"], [])
        self.assertEqual(row["topics"], [])

    def test_root_empty_parents_and_nonancestor_reference(self):
        # Reachable subtraction correctly includes the root and shared commit;
        # it does not assume the fork tip belongs to upstream's ancestry.
        self.checkpoint["reference_revision"] = self.fork
        report = history.collect(self.repo, self.checkpoint, from_reference=True)
        self.assertEqual(report["unique_commits"], 1)
        self.assertEqual(report["commits"][0]["revision"], self.upstream)
        self.checkpoint["sources"][0]["revision"] = self.base
        history.collect(self.repo, self.checkpoint)
        self.run_git("checkout", "--orphan", "unrelated")
        self.run_git("rm", "-rf", ".")
        self.checkpoint["reference_revision"] = self.commit(
            "unrelated.txt", "Other root"
        )
        report = history.collect(self.repo, self.checkpoint, from_reference=True)
        self.assertIn(self.base, [row["revision"] for row in report["commits"]])

    def test_cli_failure_and_checkpoint_validation(self):
        checkpoint = self.repo / "checkpoint.json"
        checkpoint.write_text(json.dumps(self.checkpoint))
        result = subprocess.run(
            ["python3", str(SCRIPT), str(self.repo), "--checkpoint", str(checkpoint)],
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(json.loads(result.stdout)["unique_commits"], 2)
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                str(self.repo),
                "--checkpoint",
                str(checkpoint),
                "--from-root",
                "--take",
                "1",
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        root = json.loads(result.stdout)
        self.assertEqual(root["unique_commits"], 4)
        self.assertEqual(root["commits"][0]["revision"], self.base)
        self.assertTrue(root["selection"]["has_more"])
        result = subprocess.run(
            [
                "python3",
                str(SCRIPT),
                str(self.repo),
                "--checkpoint",
                str(checkpoint),
                "--from-root",
                "--from-reference",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        checkpoint.write_text('{"version":2,"reference_revision":"bad"}')
        result = subprocess.run(
            ["python3", str(SCRIPT), str(self.repo), "--checkpoint", str(checkpoint)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("Unsupported checkpoint", result.stderr)
        for invalid in [
            [],
            {"version": 1, "reference_revision": self.base, "sources": [None]},
            {**self.checkpoint, "sources": [self.checkpoint["sources"][0]] * 2},
        ]:
            checkpoint.write_text(json.dumps(invalid))
            with self.assertRaises(history.HistoryError):
                history.read_checkpoint(checkpoint)


if __name__ == "__main__":
    unittest.main()
