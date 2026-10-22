#!/usr/bin/env python3
"""Rebuild scoped Paseo review receipts from Git objects, without checkout writes.

Supports a batch's receipts/ directory or its receipts.tar.gz. This checks
provenance and contiguous SHA positions, not the quality/completeness of review
findings, historical tests, or Fleetpane behavior.
"""

import argparse
import hashlib
import json
import re
import subprocess
import tarfile
from pathlib import Path, PurePosixPath


class ReceiptError(ValueError):
    pass


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ReceiptError(message)


def git(repository, *args):
    return subprocess.check_output(
        ["git", "-C", str(repository), *args], stderr=subprocess.PIPE, timeout=60
    )


class Receipts:
    def __init__(self, batch):
        self.batch = batch
        self.archive = None
        self.members = {}

    def __enter__(self):
        if not (self.batch / "receipts").is_dir():
            self.archive = tarfile.open(self.batch / "receipts.tar.gz", "r:gz")
            for member in self.archive:
                path = PurePosixPath(member.name)
                require(
                    not path.is_absolute()
                    and ".." not in path.parts
                    and path.parts[:1] == ("receipts",),
                    f"invalid archived receipt path: {member.name!r}",
                )
                if member.isdir():
                    continue
                require(member.isfile(), f"non-file receipt: {member.name!r}")
                require(member.name not in self.members, "duplicate archived receipt")
                self.members[member.name] = member
        return self

    def __exit__(self, *_):
        if self.archive is not None:
            self.archive.close()

    def read(self, name):
        path = PurePosixPath(name)
        require(
            not path.is_absolute()
            and ".." not in path.parts
            and path.parts[:1] == ("receipts",),
            f"invalid receipt path: {name!r}",
        )
        if self.archive is None:
            source = self.batch / name
            require(
                source.resolve().is_relative_to(self.batch.resolve()),
                f"receipt escapes batch: {name!r}",
            )
            return source.read_bytes()
        member = self.members.get(name)
        require(member is not None, f"missing archived receipt: {name!r}")
        stream = self.archive.extractfile(member)
        require(stream is not None, f"unreadable archived receipt: {name!r}")
        with stream:
            return stream.read()


def selected_patch(repository, revision, parents, ranges):
    comparison = [parents[0], revision] if len(parents) > 1 else [revision]
    if len(parents) > 1:
        raw = git(repository, "diff", "--no-ext-diff", "--no-renames", "--unified=2", *comparison)
    else:
        raw = git(repository, "show", "--format=", "--root", "--no-ext-diff", "--no-renames", "--unified=2", revision)
    blocks = {}
    for block in re.split(r"(?=^diff --git )", raw.decode(), flags=re.M):
        match = re.match(r"diff --git a/(.*?) b/(.*?)\n", block)
        if match:
            blocks[match[2]] = block.splitlines(keepends=True)
    reconstructed = ""
    for source in ranges:
        path = source["path"]
        require(path in blocks, f"source diff path missing: {path!r}")
        first, last = source["diff_block_lines"]
        require(1 <= first <= last <= len(blocks[path]), "diff range is out of bounds")
        reconstructed += (
            f"FILE {path}\nRANGE {json.dumps(source, separators=(',', ':'))}\n"
            + "".join(blocks[path][first - 1 : last])
        )
    return reconstructed.encode()


def verify(repository, batch, ledger):
    rows = json.loads((batch / "review.json").read_text())
    require(isinstance(rows, list) and bool(rows), "review must be a nonempty list")
    first, last = rows[0]["ordinal"], rows[-1]["ordinal"]
    require(
        [row["ordinal"] for row in rows] == list(range(first, last + 1)),
        "review ordinals are not contiguous",
    )
    tips = [source["target"] for source in ledger["captured_sources"]]
    pins = git(repository, "rev-list", "--reverse", "--topo-order", *tips).decode().splitlines()
    require(len(pins) == ledger["indexed_unique_commits"], "pinned DAG size changed")
    checked = supplemental = merges = 0
    with Receipts(batch) as receipts:
        for row in rows:
            ordinal, revision = row["ordinal"], row["revision"]
            require(1 <= ordinal <= len(pins), f"ordinal {ordinal} outside pinned DAG")
            require(revision == pins[ordinal - 1], f"ordinal {ordinal} SHA differs")
            parents = git(repository, "show", "-s", "--format=%P", revision).decode().split()
            tree = row["diff_tree_receipt"]
            require(parents == tree["parents"], f"{ordinal}: parents differ")
            merge = len(parents) > 1
            merges += merge
            require(tree["basis"] == ("first-parent" if merge else "parent"), f"{ordinal}: comparison differs")
            comparison = [parents[0], revision] if merge else [revision]
            expected = git(repository, "diff-tree", "--root", "--no-commit-id", "--name-status", "--no-renames", "-r", *comparison)
            raw = receipts.read(tree["file"])
            require(raw == expected and digest(raw) == tree["sha256"], f"{ordinal}: changed paths differ")
            for source in row["source_receipts"]:
                raw = receipts.read(source["file"])
                require(digest(raw) == source["sha256"], f"{ordinal}: receipt digest differs")
                if source["file"].endswith("/selected.patch"):
                    expected = selected_patch(repository, revision, parents, source["ranges"])
                    checked += 1
                else:
                    require(len(source["ranges"]) == 1, f"{ordinal}: ambiguous source receipt")
                    span = source["ranges"][0]
                    require(span["revision"] == revision, f"{ordinal}: supplemental SHA differs")
                    side = span["side"]
                    require(re.fullmatch(r"(?:\^[1-9][0-9]*|~[0-9]+)?", side) is not None, "invalid revision side")
                    blob = git(repository, "show", f"{revision}{side}:{span['path']}")
                    require(digest(blob) == source["file_blob_digest"], f"{ordinal}: source blob differs")
                    lines = blob.decode().splitlines(keepends=True)
                    start, end = span["lines"]
                    require(1 <= start <= end <= len(lines), f"{ordinal}: source range out of bounds")
                    expected = "".join(lines[start - 1 : end]).encode()
                    supplemental += 1
                require(raw == expected, f"{ordinal}: source receipt differs from Git objects")
    return {
        "review_entries": len(rows),
        "first_ordinal": first,
        "last_ordinal": last,
        "first_revision": rows[0]["revision"],
        "last_revision": rows[-1]["revision"],
        "first_parent_merge_entries": merges,
        "selected_patch_receipts_checked": checked,
        "supplemental_source_receipts_checked": supplemental,
        "review_json_sha256": digest((batch / "review.json").read_bytes()),
        "scope": "Receipt integrity only; not a full source audit or runtime/test acceptance.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repository", type=Path)
    parser.add_argument("batch", type=Path)
    parser.add_argument("--ledger", type=Path, default=Path(__file__).resolve().parents[1] / "docs/reference/paseo-replay.json")
    args = parser.parse_args()
    try:
        ledger = json.loads(args.ledger.read_text())
        result = verify(args.repository, args.batch, ledger)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError, tarfile.TarError) as error:
        parser.exit(1, f"Paseo review receipt: {error}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
