"""Publish complete JSON receipts to observers of private process fixtures."""

import json
from pathlib import Path


def publish_json(path: Path, value):
    # Fixture callers serialize writers to each path. Rename publishes only after
    # write_text closes the file; exists() must never expose an incomplete JSON.
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value))
    temporary.replace(path)
