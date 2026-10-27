# T42 · 单步失败修复与旧试跑弹窗退休

> 状态：**待开发**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**运行/修复 Agent，集成负责人串行接线**。
- 文件边界：CardRepairEntry.vue、RunHistory.vue、CardTrialPanel.vue；AutomationCenter.vue 由集成负责人。
- 开始条件：T40、T41。

## 实施清单
1. 从运行结果点修复，在原视频浮窗定位失败 stepId，打开对应编辑子窗并复用历史 snapshot 校验。
2. 修复后保存下一修订，用户显式点 ▶ 再次执行；老 run 不被修改，unknown 不自动重放。
3. 确认新单步入口涵盖准备/参数/结果/历史/修复/异常全部能力后，删除旧 CardTrialPanel 可见模态层及无用状态代码。

## 验收条件
- [ ] 同手机失败→改参数/重拾→保存→人工重试闭环，保持原视频与 owner 身份。
- [ ] 旧 trial modal 被移除后，不丢回执、清理/权限或历史定位能力。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
先替代、后删除；CardTrialPanel 的安全代码需提取复用而不是直接删掉。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
