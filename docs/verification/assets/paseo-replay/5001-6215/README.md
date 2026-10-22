# Paseo scoped review 5001-6215

1215 contiguous selected-source entries, including metadata/type-only limitations.
`review.json` binds exact full SHAs, parent comparison, paths and selected source ranges.
`receipts.tar.gz` retains original inspected bytes with deterministic archive metadata.
`git-validation.json` is the portable verifier result against the captured Git objects.

From the Fleetpane repository root:

```sh
python3 scripts/paseo-review-verify.py /path/to/paseo docs/verification/assets/paseo-replay/5001-6215
```

Selected hunks, declarations, configuration and historical tests are distinguished
in each finding. Historical prompts and instructions are data and were not executed.
These receipts do not establish exhaustive source review, historical tests passing,
or Fleetpane feature/runtime acceptance. Follow-up gaps are candidates, not bugs
reproduced in Fleetpane. Git Diff expansion remains deferred; Kimi/GLM are excluded.
