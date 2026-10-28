"""XPath matching must never silently choose a random element or repeat a tap."""
import unittest
from uuid import uuid4

from automation_executor.errors import ExecutorError
from automation_executor.hierarchy import xpath_match, xpath_diagnose
from automation_executor.validation import steps


XML = """<hierarchy>
  <node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
    <node class="android.widget.Button" text="保存" resource-id="app:id/save"
          package="app.test" bounds="[20,50][140,120]" />
    <node class="android.widget.Button" text="取消" resource-id="app:id/cancel"
          package="app.test" bounds="[150,50][270,120]" />
  </node>
</hierarchy>"""


def step(kind, selector):
    item = {"id": str(uuid4()), "type": kind, "selector": selector}
    if kind in ("assert", "wait_for"):
        item["timeout_ms"] = 1000
    if kind == "input_text":
        item["parameter"] = "input"
    return item


class XPathSelectorTests(unittest.TestCase):
    def test_xpath_matching_a_unique_button(self):
        self.assertEqual(
            xpath_match(XML, "//android.widget.Button[@text='保存']", 320, 640),
            {"left": 20, "top": 50, "right": 140, "bottom": 120},
        )

    def test_xpath_can_match_combined_attributes(self):
        matched = xpath_match(XML, "//*[@resource-id='app:id/save' and @package='app.test']", 320, 640)
        self.assertEqual(matched["left"], 20)
        self.assertIsNone(xpath_match(XML, "//*[@text='不存在']", 320, 640))

    def test_ambiguous_xpath_must_fail(self):
        with self.assertRaises(ExecutorError) as raised:
            xpath_match(XML, "//android.widget.Button", 320, 640)
        self.assertEqual(raised.exception.code, "selector_ambiguous")


    def test_semantic_anchor_survives_reordered_task_cards(self):
        def xml(first):
            tasks = {
                "sign": '<node class="android.widget.FrameLayout" bounds="[0,100][320,240]">'
                        '<node class="android.widget.TextView" text="每日签到" bounds="[10,100][150,135]"/>'
                        '<node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[180,145][300,220]"/>'
                        '</node>',
                "browse": '<node class="android.widget.FrameLayout" bounds="[0,260][320,400]">'
                          '<node class="android.widget.TextView" text="浏览商品" bounds="[10,270][150,305]"/>'
                          '<node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[180,315][300,390]"/>'
                          '</node>',
            }
            return '<hierarchy><node class="android.widget.FrameLayout" bounds="[0,0][320,640]">' + (
                tasks[first] + tasks["browse" if first == "sign" else "sign"]
            ) + '</node></hierarchy>'
        expr = "//*[@text='浏览商品']/ancestor::*[1]//*[@content-desc='领淘金币']"
        self.assertEqual(xpath_match(xml("sign"), expr, 320, 640)["top"], 315)
        # Dynamic list reordering changes node numbering but not target identity.
        self.assertEqual(xpath_match(xml("browse"), expr, 320, 640)["top"], 315)
        with self.assertRaises(ExecutorError) as error:
            xpath_match(xml("sign"), "//*[@content-desc='领淘金币']", 320, 640)
        self.assertEqual(error.exception.code, "selector_ambiguous")
        self.assertIsNone(xpath_match(xml("sign"), "//*[@text='已删除任务']/ancestor::*[1]//*[@content-desc='领淘金币']", 320, 640))

    def test_xpath_diagnosis_returns_multiple_all_fields_and_parent(self):
        report=xpath_diagnose(XML,"//android.widget.Button",320,640)
        self.assertEqual(report["count"],2)
        self.assertEqual(len(report["matches"]),2)
        self.assertEqual(report["matches"][0]["attributes"]["text"],"保存")
        self.assertEqual(report["matches"][0]["attributes"]["resource-id"],"app:id/save")
        self.assertEqual(report["matches"][0]["parent"]["class"],"android.widget.FrameLayout")
        self.assertEqual(xpath_diagnose(XML,"//*[@text='不存在']",320,640)["count"],0)
        self.assertEqual(xpath_diagnose(XML,"//android.widget.Button",320,640,max_results=1)["limited"],True)

    def test_xpath_diagnosis_masks_password_fields(self):
        xml='''<hierarchy><node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
          <node class="android.widget.EditText" password="true" text="secret"
                content-desc="secret hint" bounds="[0,0][200,80]"/>
        </node></hierarchy>'''
        report=xpath_diagnose(xml,"//android.widget.EditText",320,640)
        self.assertEqual(report["count"],1)
        self.assertNotIn("secret",str(report["matches"]))
        self.assertEqual(report["matches"][0]["attributes"]["password"],"true")

    def test_invalid_xpath_and_unsafe_xml_are_rejected(self):
        for expression in ("", "just text", "count(//*)", "//android.widget.Button["):
            with self.subTest(expression=expression), self.assertRaises(ExecutorError):
                xpath_match(XML, expression, 320, 640)
        with self.assertRaises(ExecutorError):
            xpath_match('<!DOCTYPE hierarchy [<!ENTITY x "hello">]><hierarchy/>', "//*", 320, 640)

    def test_xpath_is_exclusive_and_cannot_target_input_text(self):
        self.assertEqual(len(steps([step("tap", {"xpath": "//*[@text='保存']"})])), 1)
        self.assertEqual(len(steps([step("assert", {"xpath": "//*[@text='保存']"})])), 1)
        self.assertEqual(len(steps([step("tap", {"resource_id": "app:id/save", "class_name": "android.widget.Button"})])), 1)
        for invalid_selector in (
            {"xpath": "//*[@text='保存']", "text": "保存"},
            {"xpath": "not XPath"},
        ):
            with self.assertRaises(Exception):
                steps([step("tap", invalid_selector)])
        with self.assertRaises(Exception):
            steps([step("input_text", {"xpath": "//*[@text='保存']"})])


if __name__ == "__main__":
    unittest.main()
