# AUT-00 · UiAutomator2 / ADB 真机兼容性验收基线

适用分支：`feat/automation-center`，执行器：`automation-executor`（`uiautomator2==3.7.1`）。
**状态：代码契约测试已存在；尚未在本轮完成真机验收。** CI 的 mock RPC 不等同于 Android 17 实测。

## 前置条件

- 至少准备 Android 17 和一台其他 Android 版本手机，并记录型号、版本、分辨率、ROM、无线 ADB 端口。
- FnOS 主服务和 automation-executor 镜像必须来自兼容提交；管理台配置执行器 URL 与回调 Token。
- 需要测试手机与账号均已授权；严禁把密码、密钥、控件树里的敏感数据写入测试记录。
- 每台设备先在自动化中心查询 ADB 状态，再**主动**初始化控件组件。查询状态本身不得触发配对、安装、唤醒或触摸。

## 验收用例

| 编号 | 操作 | 预期结果 / 必留证据 |
| --- | --- | --- |
| A00-01 | 主动查询在线、离线、未授权设备的 ADB 状态 | 每台独立返回；一台超时不影响其他手机；不发送自动配对操作 |
| A00-02 | 初始化 UiAutomator2，再打开控件拾取并刷新 | 能读取层级与采集时间，控件边界与画面映射对应；查询不会重新安装组件 |
| A00-03 | 使用 Resource ID 与 XPath 定位唯一按钮，并执行点击 | 单次动作仅发送一次；多个匹配/无匹配分别明确报错 |
| A00-04 | 对测试控件长按 400ms、800ms、1500ms | 真机确实触发长按；每次只有一次 click-hold RPC；失败结果未知不得重放 |
| A00-05 | 对可拖动对象执行 300ms、700ms 拖拽 | 终点落在预期目标；与 swipe 不混淆；旋转后旧坐标步骤被阻止 |
| A00-06 | 对滚动列表执行 scroll_find | 先查找后滚动，最多 5 次；滚动方向正确，无法确认手势后停止 |
| A00-07 | 输入中文、表情、空字符串与运行时参数 | 正确定位焦点或指定输入控件；敏感参数不持久化进运行数据库 |
| A00-08 | 设备运行时断开 ADB、停止执行器、恢复网络 | 不自动重放未知结果；资源占用可核对并通过受控流程释放 |
| A00-09 | 两台设备同时执行分支不同的脚本 | Each phone 独立完成 Then/Else，跳过子条件不会执行动作，失败设备不影响其他设备 |
| A00-10 | 横屏、竖屏与投屏分辨率变化 | 截图/控件高亮位置正确；不一致时拒绝坐标操作 |

## 建议保存的验收信息

每条记录保存：用例编号、测试时间、手机型号、Android/ROM 版本、ADB 端点（脱敏）、执行器镜像摘要、主服务镜像摘要、脚本修订、步骤 ID、成功/失败及错误代码。

截图与 UI 树只在授权测试环境按需保存，保持密码过滤，并设置人工清理或明确的证据保留期限。

## 已有自动化回归

- `automation-executor/tests/test_gesture_branch.py`：单次 hold-click / drag RPC、未知结果不重试。
- `automation-executor/tests/test_advanced_steps.py`：等待消失、属性断言、有限滚动查找。
- `automation-executor/tests/test_hierarchy.py`：控件树边界、密码字段、恶意 XML。
- `automation-executor/tests/test_if_else.py`、`test_device_exit.py`：嵌套条件、跨设备分支及取消保护。
- `easytier-web/frontend/tests/automation-flow-tree.test.mjs`：树状编辑、分支删除和不可执行占位步骤。

**关闭 AUT-00 的标准：** A00-01～A00-10 的真机证据齐全且关键用例通过。在此之前状态只能标为「待真机验收」，不能因为 CI 成功而标为完成。
