"""Bounded snapshot parsing used by the interactive Android control picker."""
import unittest

from automation_executor.errors import ExecutorError
from automation_executor.hierarchy import parse_hierarchy


class HierarchyTests(unittest.TestCase):
    def test_returns_clamped_boxes_and_hides_passwords(self):
        xml = """<hierarchy rotation="0">
        <node class="android.widget.FrameLayout" bounds="[0,0][200,400]" clickable="false">
          <node class="android.widget.EditText" resource-id="com.example:id/password"
                text="do-not-return" content-desc="secret" password="true"
                bounds="[-10,20][120,80]" clickable="true" enabled="true" />
          <node class="android.widget.Button" resource-id="com.example:id/submit" text="确定"
                bounds="[20,120][180,170]" clickable="true" enabled="true" />
        </node></hierarchy>"""
        result = parse_hierarchy(xml, 200, 400)
        self.assertFalse(result["truncated"])
        self.assertEqual(len(result["nodes"]), 3)
        password = result["nodes"][1]
        self.assertEqual(password["text"], "")
        self.assertEqual(password["description"], "")
        self.assertTrue(password["password"])
        self.assertEqual(password["bounds"]["left"], 0)
        self.assertEqual(password["parent_id"], 0)
        self.assertEqual(result["nodes"][2]["text"], "确定")

    def test_rejects_entities_and_oversized_snapshot(self):
        for xml in ('<!DOCTYPE a [<!ENTITY x "hello">]><hierarchy/>', "<hierarchy>" + "a" * (512 * 1024) + "</hierarchy>"):
            with self.assertRaises(ExecutorError):
                parse_hierarchy(xml, 200, 400)

    def test_ignores_invalid_and_outside_bounds(self):
        xml = '<hierarchy><node bounds="[1,1][1,10]"/><node bounds="[300,10][400,50]"/><node bounds="[1,2][8,9]"/></hierarchy>'
        self.assertEqual(len(parse_hierarchy(xml, 200, 400)["nodes"]), 1)

    def test_only_valid_hierarchy_root(self):
        with self.assertRaises(ExecutorError):
            parse_hierarchy("<payload/>", 200, 400)


if __name__ == "__main__":
    unittest.main()
