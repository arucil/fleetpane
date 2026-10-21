# Complete ACP review fixture receipts

The exact `ab6b0da7aad7d807ad59733aea9fa85ddf02ad24` CI run
[39649983939](https://github.com/arucil/fleetpane/actions/runs/39649983939)
passed Linux desktop/core and Windows core. macOS core failed in
`acp-review-smoke.py` while decoding an empty answer receipt. The fixture created
the final path with `write_text`; its observer waited only for `exists()`. File
creation therefore preceded complete publication. This is a fixture race, not
evidence of a provider-answer or desktop projection defect.

The fixture now writes and closes a sibling staging file before renaming it to
the answer path. A deterministic threaded test pauses immediately after opening
the writer: the first receipt stays absent and an existing receipt retains its
previous complete generation. It then checks exact JSON and staging cleanup.
Substituting the old direct-write behavior fails both controls (the first exposes
an empty file; the replacement truncates the previous receipt).

Local Linux checks:

- `python3 -m unittest discover -s scripts/tests -p '*_test.py'`: 16 passed.
- `python3 scripts/acp-review-smoke.py`: exact answers, stale controllers,
  independent permission and archive behavior passed.
- `git diff --check`: passed.

This changes private synthetic fixture publication only. macOS acceptance of the
fix requires a new exact-head CI run; local Linux success is a separate result.
