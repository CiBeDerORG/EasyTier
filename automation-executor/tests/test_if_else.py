"""If/Else branch layout and read-only condition tests."""
import unittest
from uuid import uuid4

from automation_executor.validation import steps
from test_advanced_steps import ActionHarness, BUTTON


def item(kind, **kwargs):
    return {"id": str(uuid4()), "type": kind, **kwargs}


def key():
    return item("key", key="HOME")


class IfElseValidationTests(unittest.TestCase):
    def test_valid_then_else_with_trailing_action(self):
        flow = [item("if_else", selector=BUTTON, then_count=2, else_count=1),
                key(), key(), key(), key()]
        self.assertEqual(len(steps(flow)), 5)

    def test_rejects_nonexistent_and_oversized_branches(self):
        invalid = [
            [item("if_else", selector=BUTTON, then_count=1, else_count=1), key()],
            [item("if_else", selector=BUTTON, then_count=0, else_count=1), key(), key()],
            [item("if_else", selector=BUTTON, then_count=21, else_count=1)]
                + [key() for _ in range(23)],
            [item("if_else", selector=BUTTON, then_count=1, else_count=1),
             item("if_gone", selector=BUTTON, skip_count=1), key()],
            # A child condition cannot cross its parent's Then/Else boundary.
            [item("if_else", selector=BUTTON, then_count=2, else_count=1),
             key(), item("if_gone", selector=BUTTON, skip_count=1), key()],
        ]
        for flow in invalid:
            with self.subTest(flow=[x["type"] for x in flow]), self.assertRaises(Exception):
                steps(flow)

    def test_three_level_nested_conditions_are_valid(self):
        flow = [
            item("if_else", selector=BUTTON, then_count=5, else_count=1),
            item("if_gone", selector=BUTTON, skip_count=4),
            item("if_else", selector=BUTTON, then_count=1, else_count=2),
            key(), key(), key(), key(),
        ]
        self.assertEqual(len(steps(flow)), len(flow))

    def test_fourth_nested_level_is_blocked(self):
        flow = [
            item("if_gone", selector=BUTTON, skip_count=5),
            item("if_gone", selector=BUTTON, skip_count=4),
            item("if_gone", selector=BUTTON, skip_count=3),
            item("if_gone", selector=BUTTON, skip_count=2),
            key(), key(),
        ]
        with self.assertRaises(Exception):
            steps(flow)

    def test_legacy_if_can_contain_if_else_when_fully_nested(self):
        flow = [
            item("if_gone", selector=BUTTON, skip_count=3),
            item("if_else", selector=BUTTON, then_count=1, else_count=1),
            key(), key(),
        ]
        self.assertEqual(len(steps(flow)), len(flow))

    def test_disjoint_branches_are_accepted(self):
        flow = [item("if_else", selector=BUTTON, then_count=1, else_count=1), key(), key(),
                item("if_else", selector=BUTTON, then_count=1, else_count=1), key(), key()]
        self.assertEqual(len(steps(flow)), len(flow))


class IfElseExecutionTests(unittest.TestCase):
    def test_if_else_returns_explicit_branch_from_one_read_only_check(self):
        for count, expected in ((0, "then"), (1, "else"), (2, "else")):
            runner = ActionHarness(count=lambda sel: 1 if sel.values.get("className") else count)
            result = runner.run(item("if_else", selector=BUTTON, then_count=2, else_count=1))
            self.assertEqual(result["branch_taken"], expected)
            self.assertEqual(result["state"], "condition_passed")
            self.assertEqual([name for name, _ in runner.calls], ["count", "count"])

    def test_rpc_failure_is_not_assumed_absent(self):
        runner = ActionHarness(count=lambda sel: 1 if sel.values.get("className") else None)
        with self.assertRaises(Exception):
            runner.run(item("if_else", selector=BUTTON, then_count=1, else_count=1))


if __name__ == "__main__":
    unittest.main()
