# T17 · AC01–AC12 自动测试覆盖矩阵（代码提交不等于实测通过）

基线：`require/require-01-acceptance.md`。本表只记录与用户验收对应的测试代码及验证状态；模拟手机与 SQLite 单测无法代替 Android/FnOS 和浏览器完整流程。

| AC | 已提交自动测试及对应目标 | 无法由这些测试证明的项目 | 进度 |
|---|---|---|---|
| AC-01 | `automation-card-workflow-t17.test.mjs` 拾取 preparing/on/off、切 session 和 generation 隔离 | WebRTC 真画面、按钮可见/可点击 | 代码已提交 |
| AC-02 | 同文件模拟视频有效区域、黑边之外拒绝、方向/比例突变拒绝 | 原生鼠标右键菜单事件、手机长按和全屏 | 代码已提交 |
| AC-03 | 同文件 `matchPickerPoint` 唯一候选；现有 `automation-locator-candidates.test.mjs` 树截断/密码/重复控件 | UiAutomator2 实机属性树与 UI 操作 | 代码已提交 |
| AC-04 | 同文件 `appendPickedAutomationStep` 同一 ID 二次确认不得新增 | Vue 对话框关闭和连续拾取保持 | 代码已提交 |
| AC-05 | 同文件在共享工作草稿生成四步并核查顺序/ID | 连续右键的真实浏览器交互 | 代码已提交 |
| AC-06 | 同文件 `projectCardSteps` 步骤序号和新草稿状态 | Vue 卡片真实状态显示与自动保存回显 | 代码已提交 |
| AC-07 | 同文件 `updateCardDraftStep` CAS，`replaceAutomationStepTarget` 原步骤 ID/未编辑步骤保持 | 回卡片完整编辑器、位置复拾操作 | 代码已提交 |
| AC-08 | 同文件 `locateReadOnlyStep` one/none/many/stale，不接 Android 动作 API | 手机画面真实高亮、层级与渲染坐标 | 代码已提交 |
| AC-09 | `automation-run-safety-t17.test.mjs` 预检未准备、预览会话归属/清理；Python SQLite journal 双向资源排他 | ADB 断连重连、UiAutomator2 实际启动及进度 UI | 代码已提交 |
| AC-10 | 同文件用户确认单步、历史 snapshot/revision fork；现有 `automation-run-repair.test.mjs` 与真实模拟旧快照 | 真机失败回步、参数修改、重新运行效果 | 代码已提交 |
| AC-11 | `automation-device-compatibility-t17.test.mjs` 设备绑定、屏幕尺寸/方向、Selector 唯一性、过期快照；Rust draft receipt | 已保存草稿重新打开/另一授权手机实机试跑 | 代码已提交 |
| AC-12 | `automation-card-workflow-t17.test.mjs` 模拟投屏断开不清空步骤；Rust 请求 ID/修订隔离 | 浏览器路由离开、网络保存未知、两标签页 CAS 冲突和硬刷新 | 代码已提交 |

## 层级与执行证据
- **Node 层**：仓库原有 `test:unit` 已加入三个 T17 新文件。GitHub Actions `40495290119` 首次运行中，125 个用例 124 个通过、1 个测试断言问题；`3d6b6d3` 已修正该断言，等待新提交的 CI。
- **Python 层**：新增 `test_trial_handoff_t17.py`，使用执行器真正的 SQLite Journal，覆盖手控与脚本独占及 cleanup_pending 重启恢复。尚未确认被 CI 执行，禁止宣称通过。
- **Rust 层**：`drafts.rs::t17_draft_regressions` 直接覆盖 UUID、revision、receipt 和隔离；未进行单独 `cargo test`，仍待编译与测试证据。
- **浏览器与真机层**：尚未执行，按 T18 测试场景逐项取证。

## 待处理的最低验证门禁
1. 对应提交的完整 Node 单测必须通过；不得引用前一提交的通过结果。
2. 由获授权人员执行 Python 执行器回归及 Rust 后端测试，核对无模拟之外的副作用。
3. 在 FnOS Web 管理页面+至少两台 Android + 两个浏览器标签页运行 T18 实测，核查 AC01–AC12 和失败案例。
4. 原生右键/触屏、真实 ADB/UiAutomator2 与 SQLite 竞争只能由实测关闭，不因 T17 单测通过自动标记 AC 已验收。
