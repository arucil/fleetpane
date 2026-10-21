"""Regression for observer startup racing completion, without sleeps or a model."""

import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "concurrency", Path(__file__).resolve().parents[1] / "concurrency-smoke.py"
)
concurrency = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(concurrency)


class ConcurrencyObserverTest(unittest.TestCase):
    def test_completion_during_startup_remains_after_the_baseline(self):
        events = []

        def snapshot():
            return {"sequence": len(events)}

        def operation(method, params):
            self.assertEqual(method, "turn/start")
            thread = params["threadId"]
            turn = {"id": f"turn-{thread}", "status": "completed"}
            events.append(
                {
                    "sequence": len(events) + 1,
                    "message": {
                        "method": "turn/completed",
                        "params": {"threadId": thread, "turn": turn},
                    },
                }
            )
            return {"turn": turn}

        boundary, targets = concurrency.start_turns(["a", "b"], operation, snapshot)
        delivered = [event for event in events if event["sequence"] > boundary]
        self.assertEqual(concurrency.completed_targets(delivered, targets), {"a", "b"})

        consumed = set()

        def consume(boundary, targets):
            delivered = [event for event in events if event["sequence"] > boundary]
            consumed.update(concurrency.completed_targets(delivered, targets))
            return events[-1]["sequence"]

        events.clear()
        boundary, _ = concurrency.start_turns(["a", "b"], operation, snapshot, consume)
        self.assertEqual(consumed, {"a", "b"})
        self.assertEqual(boundary, 2)

    def test_completion_must_match_both_thread_and_turn(self):
        def completion(thread, turn, status="completed"):
            return {
                "message": {
                    "method": "turn/completed",
                    "params": {
                        "threadId": thread,
                        "turn": {"id": turn, "status": status},
                    },
                }
            }

        targets = {"a": "current"}
        self.assertEqual(
            concurrency.completed_targets(
                [completion("a", "previous"), completion("other", "current")], targets
            ),
            set(),
        )
        self.assertEqual(
            concurrency.completed_targets(
                [completion("a", "current", "interrupted")], targets
            ),
            {"a"},
        )
        with self.assertRaises(AssertionError):
            concurrency.completed_targets(
                [completion("a", "current", "inProgress")], targets
            )
