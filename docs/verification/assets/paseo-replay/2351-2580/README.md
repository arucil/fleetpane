# Paseo scoped review 2351–2580

230 contiguous scoped reviews; 20 first-parent merges and five supplemental
source ranges. `review.json` describes the exact inspected paths/ranges.
`receipts.tar.gz` preserves their original bytes and metadata with deterministic
archive timestamps. Historical prompts/instructions are reference data.

From the Fleetpane repository root:

```sh
python3 scripts/paseo-review-verify.py /path/to/paseo docs/verification/assets/paseo-replay/2351-2580
```

The portable verifier derives pinned full SHAs from the ledger's captured tips
and reconstructs selected hunks from Git without reading dirty worktree files.
It supports both unpacked receipts and the archive without extraction. The
archived `inspect.py`/`source.py`/`record.py`/`verify.py` retain the original local
capture procedure; their `/tmp` paths are historical and need not be recreated.

Passing provenance checks do not prove historical tests passed, every changed
path was audited, or Fleetpane implements these workflows. Native acceptance is
recorded independently.

Validation also passed the portable verifier against all 230 records; the verifier
regression tests cover recomputed-digest tampering, wrong SHA/comparison, unsafe
archive entries and exact supplemental source range binding.
