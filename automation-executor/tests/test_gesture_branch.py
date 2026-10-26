"""Closed-schema and single-shot gesture/conditional regression tests."""
import unittest
from uuid import uuid4

from automation_executor.steps import StepFailure
from automation_executor.validation import steps
from test_advanced_steps import ActionHarness, BUTTON


def step(kind, **kwargs):
    return {"id": str(uuid4()), "type": kind, **kwargs}


class GesturesAndBranchSchema(unittest.TestCase):
    def test_accept_bounded_gestures_and_forward_conditional(self):
        valid = [
            step("long_press", selector=BUTTON, duration_ms=700),
            step("long_press", x=.5, y=.4, width=320, height=640, duration_ms=800),
            step("drag", x=.2, y=.3, to_x=.8, to_y=.7, width=320, height=640, duration_ms=1200),
            step("if_gone", selector=BUTTON, skip_count=1),
            step("key", key="BACK"),
        ]
        self.assertEqual(len(steps(valid)), 5)

    def test_reject_infinite_or_malformed_branch_and_gestures(self):
        bad = [
            [step("if_gone", selector=BUTTON, skip_count=1)],
            [step("if_gone", selector=BUTTON, skip_count=21)] + [step("key", key="BACK") for _ in range(22)],
            [step("if_gone", selector=BUTTON, skip_count=0), step("key", key="BACK")],
            [step("long_press", selector=BUTTON, duration_ms=200)],
            [step("long_press", selector=BUTTON, x=.4, y=.4, width=320, height=640, duration_ms=800)],
            [step("drag", x=.5, y=.5, to_x=.8, to_y=.7, width=320, height=640, duration_ms=100)],
        ]
        for bundle in bad:
            with self.subTest(case=bundle[0]["type"]), self.assertRaises(Exception):
                steps(bundle)


class GestureExecution(unittest.TestCase):
    def test_selector_long_press_is_a_single_hold_click(self):
        runner = ActionHarness(
            info=lambda s: {"bounds": {"left": 20, "top": 30, "right": 140, "bottom": 130}})
        result = runner.run(step("long_press", selector=BUTTON, duration_ms=650))
        self.assertEqual(result["state"], "action_sent")
        calls = [params for method, params in runner.calls if method == "click"]
        self.assertEqual(calls, [(80, 80, 650)])

    def test_drag_uses_single_drag_rpc(self):
        runner = ActionHarness()
        result = runner.run(step("drag", x=.25, y=.25, to_x=.75, to_y=.75,
                                  width=320, height=640, duration_ms=600))
        self.assertEqual(result["state"], "action_sent")
        self.assertEqual([method for method, _ in runner.calls].count("drag"), 1)
        self.assertEqual([params for method, params in runner.calls if method == "drag"], [(80, 160, 240, 480, 120)])

    def test_gesture_transport_exception_is_unknown_and_not_retried(self):
        runner = ActionHarness(on_swipe=lambda: (_ for _ in ()).throw(ConnectionError("unknown")))
        with self.assertRaises(StepFailure) as failed:
            runner.run(step("drag", x=.2, y=.2, to_x=.8, to_y=.8, width=320, height=640, duration_ms=500))
        self.assertEqual(failed.exception.code, "device_action_unconfirmed")
        self.assertTrue(failed.exception.outcome_unknown)
        self.assertEqual(sum(method == "drag" for method, _ in runner.calls), 1)

    def test_if_gone_is_read_only_and_presence_controls_skip(self):
        for matching, skip in ((0, 0), (1, 2), (2, 2)):
            runner = ActionHarness(count=lambda s: 1 if s.values.get("className") else matching)
            result = runner.run(step("if_gone", selector=BUTTON, skip_count=2))
            self.assertEqual(result["state"], "condition_passed")
            self.assertEqual(result["skip_steps"], skip)
            self.assertEqual({method for method, _ in runner.calls}, {"count"})

    def test_if_gone_fails_closed_on_invalid_rpc(self):
        runner = ActionHarness(count=lambda s: 1 if s.values.get("className") else None)
        with self.assertRaises(StepFailure):
            runner.run(step("if_gone", selector=BUTTON, skip_count=1))


if __name__ == "__main__":
    unittest.main()
