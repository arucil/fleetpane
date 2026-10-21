#!/usr/bin/env python3
"""Read Paseo history from its roots or captured checkpoints (no fetch/write)."""

import argparse
import json
import re
import subprocess
from pathlib import Path

CHECKPOINT = Path(__file__).resolve().parents[1] / "docs/reference/paseo-history.json"
SHA = re.compile(r"[0-9a-f]{40}\Z")
TOPICS = {
    "files": ("/file-explorer/", "/file-pane/", "file-edit", "file-change-conflict"),
    "streaming-and-navigation": (
        "/timeline/",
        "/replica-cache/",
        "/sidebar/",
        "/subagents/",
    ),
    "providers": (
        "/providers/",
        "/agent/agent-",
        "acp-agent",
        "/executable-resolution/",
    ),
    "workspaces": ("/workspace", "/worktree", "/projects/", "/project/"),
    "terminal": ("/terminal", "terminal-"),
    "composer-and-drafts": ("/composer/", "/draft-", "draft-store"),
    "transport": ("/relay/", "/hub/", "daemon-client", "websocket", "/protocol/"),
}


class HistoryError(Exception):
    pass


def git(repo, *args):
    result = subprocess.run(
        ["git", "--no-optional-locks", "-C", str(repo), *args],
        capture_output=True,
        timeout=30,
        check=False,
    )
    if result.returncode:
        raise HistoryError(result.stderr.decode("utf-8", "replace").strip())
    return result.stdout


def revision(repo, name):
    return (
        git(repo, "rev-parse", "--verify", "--end-of-options", name + "^{commit}")
        .decode()
        .strip()
    )


def read_checkpoint(path):
    data = json.loads(path.read_text())
    if (
        not isinstance(data, dict)
        or data.get("version") != 1
        or not isinstance(data.get("reference_revision"), str)
        or not SHA.fullmatch(data["reference_revision"])
    ):
        raise HistoryError("Unsupported checkpoint or invalid reference revision")
    sources = data.get("sources")
    if not isinstance(sources, list) or not sources:
        raise HistoryError("Checkpoint must contain named sources")
    names = set()
    for source in sources:
        if (
            not isinstance(source, dict)
            or not all(
                isinstance(source.get(key), str)
                for key in ("name", "ref", "revision", "url")
            )
            or not source["name"]
            or source["name"] in names
            or not source["ref"].startswith("refs/remotes/")
            or not SHA.fullmatch(source["revision"])
            or not re.fullmatch(
                r"https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", source["url"]
            )
        ):
            raise HistoryError(
                "Invalid or duplicate source name, ref, revision or public URL"
            )
        names.add(source["name"])
    return data


def collect(repo, checkpoint, from_reference=False, limit=1000, *, from_root=False):
    if from_root and from_reference:
        raise HistoryError("Choose root history or reference delta, not both")
    if git(repo, "rev-parse", "--is-shallow-repository").strip() != b"false":
        raise HistoryError(
            "Full Git history required; a shallow clone can omit ancestors"
        )
    sources = []
    unique = {}
    for source in checkpoint["sources"]:
        git(repo, "check-ref-format", source["ref"])
        target = revision(repo, source["ref"])
        baseline = (
            None
            if from_root
            else (
                checkpoint["reference_revision"]
                if from_reference
                else source["revision"]
            )
        )
        excluded = [] if baseline is None else ["--not", baseline]
        if baseline is not None:
            revision(repo, baseline)
        # The original reference is a fork merge, so reference mode intentionally
        # uses reachable-set subtraction. Incremental mode refuses rewritten refs.
        if baseline is not None and not from_reference:
            ancestor = git(repo, "merge-base", baseline, target).decode().strip()
            if ancestor != baseline:
                raise HistoryError(
                    f"{source['name']}: checkpoint is not an ancestor; review ref rewrite"
                )
        count = int(git(repo, "rev-list", "--count", target, *excluded))
        if count > limit:
            raise HistoryError(
                f"{source['name']}: {count} commits exceed --limit {limit}; no partial report"
            )
        encoded = git(
            repo,
            "log",
            "--reverse",
            "--topo-order",
            "-z",
            "--format=%H%x00%cI%x00%s%x00%P",
            target,
            *excluded,
        )
        fields = encoded[:-1].split(b"\0") if encoded.endswith(b"\0") else []
        if len(fields) != 4 * count:
            raise HistoryError("Malformed Git metadata; report was not generated")
        for index in range(0, len(fields), 4):
            commit, date, subject, parents = (
                part.decode("utf-8") for part in fields[index : index + 4]
            )
            if commit not in unique:
                merge = len(parents.split()) > 1
                paths = (
                    []
                    if merge
                    else [
                        path.decode("utf-8", "surrogateescape")
                        for path in git(
                            repo,
                            "diff-tree",
                            "--root",
                            "--no-commit-id",
                            "-r",
                            "--name-only",
                            "--no-renames",
                            "-z",
                            commit,
                        ).split(b"\0")
                        if path
                    ]
                )
                unique[commit] = {
                    "revision": commit,
                    "date": date,
                    "subject": subject,
                    "parents": parents.split(),
                    "merge": merge,
                    "paths": paths,
                    "topics": [
                        name
                        for name, patterns in TOPICS.items()
                        if any(
                            pattern in path for path in paths for pattern in patterns
                        )
                    ],
                    "sources": [],
                    "links": [],
                }
            unique[commit]["sources"].append(source["name"])
            unique[commit]["links"].append(f"{source['url']}/commit/{commit}")
        scope = {
            "name": source["name"],
            "ref": source["ref"],
            "baseline": baseline,
            "target": target,
            "commits_in_scope": count,
            "roots": git(repo, "rev-list", "--max-parents=0", target)
            .decode()
            .splitlines(),
        }
        if not from_root:
            scope["new_commits"] = count
        sources.append(scope)
    # A single parent-before-child order across both lineages. Appending the
    # fork-only commits after the entire upstream list is not a historical replay.
    ordered = (
        git(
            repo,
            "rev-list",
            "--reverse",
            "--topo-order",
            *(s["target"] for s in sources),
        )
        .decode()
        .splitlines()
    )
    commits = [unique[commit] for commit in ordered if commit in unique]
    for ordinal, commit in enumerate(commits, 1):
        commit["ordinal"] = ordinal
    return {
        "version": 1,
        "mode": "from-root"
        if from_root
        else ("since-reference" if from_reference else "since-checkpoint"),
        "head": revision(repo, "HEAD"),
        "worktree_dirty": bool(
            git(repo, "status", "--porcelain", "-z", "--untracked-files=normal")
        ),
        "sources": sources,
        "unique_commits": len(unique),
        "commits": commits,
    }


def batch(report, after=None, take=None):
    """Select an explicit review batch without misrepresenting it as the full DAG."""
    if after is None and take is None:
        return report
    if report["mode"] != "from-root":
        raise HistoryError("Review batches require --from-root")
    if take is not None and not 1 <= take <= 1000:
        raise HistoryError("--take must be between 1 and 1000")
    commits = report["commits"]
    start = 0
    if after is not None:
        matches = [
            index for index, commit in enumerate(commits) if commit["revision"] == after
        ]
        if not SHA.fullmatch(after) or len(matches) != 1:
            raise HistoryError(
                "--after must be a full commit SHA present in this captured history"
            )
        start = matches[0] + 1
    selected = commits[start:] if take is None else commits[start : start + take]
    return {
        **report,
        "commits": selected,
        "selection": {
            "after": after,
            "take": take,
            "returned": len(selected),
            "has_more": start + len(selected) < len(commits),
            "next_after": selected[-1]["revision"] if selected else after,
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    scope = parser.add_mutually_exclusive_group()
    scope.add_argument(
        "--from-root",
        action="store_true",
        help="Include every commit, starting at each reachable root",
    )
    parser.add_argument(
        "repository", type=Path, help="Full Paseo checkout; dirty files are never read"
    )
    parser.add_argument("--checkpoint", type=Path, default=CHECKPOINT)
    parser.add_argument(
        "--after", help="Root review batch: start after this full commit SHA"
    )
    parser.add_argument(
        "--take",
        type=int,
        help="Root review batch: explicitly return this many commits, up to 1000",
    )
    scope.add_argument(
        "--from-reference",
        action="store_true",
        help="Compare with the original fork reference",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=1000,
        help="Maximum commits per source; excess is an error",
    )
    args = parser.parse_args()
    if not 1 <= args.limit <= 10000:
        parser.error("--limit must be between 1 and 10000")
    try:
        report = collect(
            args.repository,
            read_checkpoint(args.checkpoint),
            args.from_reference,
            args.limit,
            from_root=args.from_root,
        )
        report = batch(report, args.after, args.take)
    except (
        HistoryError,
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.TimeoutExpired,
    ) as error:
        parser.exit(1, f"Paseo history: {error}\n")
    print(json.dumps(report, indent=2, ensure_ascii=True))


if __name__ == "__main__":
    main()
