# T41 · 单步状态、执行记录及原画面反馈

> 状态：**步骤行状态组件已接线，时间和异常回归待补齐**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**运行 Agent**。
- 文件边界：新 automation/InlineStepRunStatus.vue；复用 useCardRunStatus.ts、automation-trial-receipt.ts。
- 开始条件：T40。

## 实施清单
1. 步骤行显示排队/检查中/准备中/执行中/动作已发送/断言通过/失败/结果未知及运行时间。
2. 关联 runId/deviceId/stepId/requestId 与 immutable step snapshot，结果未知停在可查询态。
3. 主窗或子窗结果显示不会阻塞视频观看，也不能把只读定位演示当成真实执行。

## 验收条件
- [ ] 运行一步显示准确进度，失败/结果未知能打开关联记录。
- [ ] 切换全屏、拖动窗口或折叠脚本列表后 runId 不丢、不会重发手机动作。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
保留执行器可靠回执，禁止 UI 定时刷新隐含重复 POST。

## 验证状态
相关代码已提交，但尚未测试或构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T41 第一批进度交付（调试模式）

- 新增 `components/automation/InlineStepRunStatus.vue`，在唯一 `CardStepTimeline.vue` 的对应 stepId 行显示“准备/待确认/提交/执行/动作已发送/完成/失败/取消/未知”等状态、运行 ID 前 8 位及当前请求 ID 悬浮提示。
- `CardTrialPanel.vue` 只读投影原 `useCardRunStatus` 的运行回执，携带 ownerId/deviceId/stepId/requestId/runId，使用 `progress` 事件交给同一 PhoneCard；拖动子浮窗或进入全屏不切换原运行记录，也不会额外 POST。
- `PhoneCard.vue` 仅在本卡片当前试跑 stepId 与 owner/device 对齐时接收投影；授权 owner 变化清理上次本地进度。已有历史/修复入口仍在浮窗保留。
- 待补：本次试跑耗时与服务端 started_at/finished_at 映射、回显更多步骤/设备信息、结果未知后专门的就地只读重查 UX、真实断线/关闭窗口回归；T42 失败修复与历史模态退役尚未完成。
- **无测试、构建、手动 CI、浏览器或 FnOS/Android 真机验收**；验收框仍未勾选。
