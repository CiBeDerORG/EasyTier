# T41 · 单步状态、执行记录及原画面反馈

> 状态：**待开发**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
