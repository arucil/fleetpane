"""Receipt tampering is rejected even when its claimed digest is recomputed."""

import importlib.util
import io
import json
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "paseo-review-verify.py"
SPEC = importlib.util.spec_from_file_location("paseo_review_verify", SCRIPT)
review = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(review)


class ReviewReceiptTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="fleetpane-review-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repo = self.root / "source"
        self.repo.mkdir()
        self.git("init", "--initial-branch=main")
        self.git("config", "user.name", "Synthetic Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.source = self.repo / "fixture.txt"
        self.source.write_text("Original 中文\n")
        self.git("add", "fixture.txt")
        self.git("-c", "commit.gpgsign=false", "commit", "-m", "root")
        self.base = self.git("rev-parse", "HEAD").decode().strip()
        self.source.write_text("New 中文\n")
        self.git("add", "fixture.txt")
        self.git("-c", "commit.gpgsign=false", "commit", "-m", "edit")
        self.head = self.git("rev-parse", "HEAD").decode().strip()
        self.batch = self.root / "batch"
        self.receipts = self.batch / "receipts/0002"
        self.receipts.mkdir(parents=True)
        self.patch = self.git("show", "--format=", "--root", "--no-ext-diff", "--no-renames", "--unified=2", self.head)
        self.span = {"path": "fixture.txt", "diff_block_lines": [1, len(self.patch.decode().splitlines())]}
        content = f"FILE fixture.txt\nRANGE {json.dumps(self.span, separators=(',', ':'))}\n".encode() + self.patch
        (self.receipts / "selected.patch").write_bytes(content)
        paths = self.git("diff-tree", "--root", "--no-commit-id", "--name-status", "--no-renames", "-r", self.head)
        (self.receipts / "paths.txt").write_bytes(paths)
        self.rows = [{
            "ordinal": 2, "revision": self.head,
            "diff_tree_receipt": {"file": "receipts/0002/paths.txt", "sha256": review.digest(paths), "basis": "parent", "parents": [self.base]},
            "source_receipts": [{"file": "receipts/0002/selected.patch", "sha256": review.digest(content), "ranges": [self.span]}],
        }]
        self.ledger = {"captured_sources": [{"target": self.head}], "indexed_unique_commits": 2}
        self.save()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], stderr=subprocess.PIPE)

    def save(self):
        (self.batch / "review.json").write_text(json.dumps(self.rows))

    def test_git_reconstruction_accepts_directory_and_archive_without_checkout_edits(self):
        before = (self.repo / ".git/index").read_bytes()
        self.source.write_text("Dirty user data, never inspected\n")
        result = review.verify(self.repo, self.batch, self.ledger)
        self.assertEqual(result["selected_patch_receipts_checked"], 1)
        with tarfile.open(self.batch / "receipts.tar.gz", "w:gz") as archive:
            for path in self.receipts.iterdir():
                archive.add(path, arcname=str(path.relative_to(self.batch)))
        (self.batch / "receipts").rename(self.batch / "detached")
        self.assertEqual(review.verify(self.repo, self.batch, self.ledger), result)
        self.assertEqual((self.repo / ".git/index").read_bytes(), before)
        self.assertEqual(self.source.read_text(), "Dirty user data, never inspected\n")

    def test_recomputed_digest_cannot_hide_changed_source_receipt(self):
        path = self.receipts / "selected.patch"
        path.write_bytes(path.read_bytes().replace(b"New", b"Invented"))
        self.rows[0]["source_receipts"][0]["sha256"] = review.digest(path.read_bytes())
        self.save()
        with self.assertRaisesRegex(review.ReceiptError, "differs from Git"):
            review.verify(self.repo, self.batch, self.ledger)

    def test_pinned_sha_and_comparison_basis_are_checked(self):
        self.rows[0]["revision"] = self.base
        self.save()
        with self.assertRaisesRegex(review.ReceiptError, "SHA differs"):
            review.verify(self.repo, self.batch, self.ledger)
        self.rows[0]["revision"] = self.head
        self.rows[0]["diff_tree_receipt"]["basis"] = "first-parent"
        self.save()
        with self.assertRaisesRegex(review.ReceiptError, "comparison differs"):
            review.verify(self.repo, self.batch, self.ledger)

    def test_archives_are_never_extracted_and_refuse_duplicate_or_escaping_paths(self):
        (self.batch / "receipts").rename(self.batch / "detached")
        for names in [["../outside"], ["receipts/duplicate", "receipts/duplicate"]]:
            with self.subTest(names=names):
                with tarfile.open(self.batch / "receipts.tar.gz", "w:gz") as archive:
                    for name in names:
                        info = tarfile.TarInfo(name)
                        info.size = 1
                        archive.addfile(info, io.BytesIO(b"x"))
                with self.assertRaises(review.ReceiptError):
                    review.verify(self.repo, self.batch, self.ledger)
                self.assertFalse((self.root / "outside").exists())

    def test_supplemental_parent_byte_receipt_binds_exact_span(self):
        data = self.git("show", f"{self.base}:fixture.txt")
        excerpt = data[2:9]
        (self.receipts / "source-01.txt").write_bytes(excerpt)
        source = {
            "file": "receipts/0002/source-01.txt", "sha256": review.digest(excerpt),
            "file_blob_digest": review.digest(data),
            "ranges": [{"revision": self.head, "side": "^", "path": "fixture.txt", "lines": None, "bytes": [2, 9]}],
        }
        self.rows[0]["source_receipts"].append(source)
        self.save()
        self.assertEqual(review.verify(self.repo, self.batch, self.ledger)["supplemental_source_receipts_checked"], 1)
        source["ranges"][0]["byte_range_half_open"] = source["ranges"][0].pop("bytes")
        source["ranges"][0]["side"] = "^1"
        self.save()
        self.assertEqual(review.verify(self.repo, self.batch, self.ledger)["supplemental_source_receipts_checked"], 1)
        source["ranges"][0]["bytes"] = source["ranges"][0].pop("byte_range_half_open")
        source["ranges"][0]["bytes"] = [0, len(data) + 1]
        self.save()
        with self.assertRaisesRegex(review.ReceiptError, "byte range out of bounds"):
            review.verify(self.repo, self.batch, self.ledger)

    def test_supplemental_receipt_binds_exact_blob_and_line_range(self):
        data = self.git("show", f"{self.head}:fixture.txt")
        (self.receipts / "source-01.txt").write_bytes(data)
        self.rows[0]["source_receipts"].append({
            "file": "receipts/0002/source-01.txt", "sha256": review.digest(data),
            "file_blob_digest": review.digest(data),
            "ranges": [{"revision": self.head, "side": "", "path": "fixture.txt", "lines": [1, 1]}],
        })
        self.save()
        self.assertEqual(review.verify(self.repo, self.batch, self.ledger)["supplemental_source_receipts_checked"], 1)
        self.rows[0]["source_receipts"][-1]["ranges"][0]["lines"] = [1, 2]
        self.save()
        with self.assertRaisesRegex(review.ReceiptError, "source range out of bounds"):
            review.verify(self.repo, self.batch, self.ledger)


if __name__ == "__main__":
    unittest.main()
