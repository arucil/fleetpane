"""Readers must see no receipt (or its previous generation) until publication."""

import importlib.util
import json
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "fixture_files.py"
SPEC = importlib.util.spec_from_file_location("fixture_files", SCRIPT)
fixture_files = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture_files)


class PublicationTest(unittest.TestCase):
    def test_readers_cannot_observe_a_partially_written_answer(self):
        for previous in (None, {"outcome": "previous"}):
            with self.subTest(previous=previous), tempfile.TemporaryDirectory() as directory:
                target = Path(directory) / "answer.json"
                if previous is not None:
                    target.write_text(json.dumps(previous))
                opened = threading.Event()
                finish = threading.Event()
                failures = []

                def delayed_write(path, text):
                    with path.open("w") as output:
                        opened.set()
                        if not finish.wait(5):
                            raise TimeoutError("Reader never released the fixture writer")
                        output.write(text)

                expected = {"outcome": {"outcome": "selected", "optionId": "allow-one"}}

                def publish():
                    try:
                        fixture_files.publish_json(target, expected)
                    except Exception as error:
                        failures.append(error)

                with patch.object(Path, "write_text", delayed_write):
                    worker = threading.Thread(target=publish)
                    worker.start()
                    try:
                        self.assertTrue(opened.wait(5), "Writer did not open the staging file")
                        if previous is None:
                            self.assertFalse(target.exists(), "Reader sees an incomplete receipt")
                        else:
                            self.assertEqual(json.loads(target.read_text()), previous)
                    finally:
                        finish.set()
                        worker.join(5)
                self.assertFalse(worker.is_alive())
                self.assertEqual(failures, [])
                self.assertEqual(json.loads(target.read_text()), expected)
                self.assertFalse(target.with_suffix(".json.tmp").exists())
