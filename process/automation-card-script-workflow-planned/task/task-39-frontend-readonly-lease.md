# T39 · 前端只读观看与执行态输入屏蔽

> 状态：**待开发**｜计划：P12｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**卡片 Agent + 集成负责人按文件串行**。
- 文件边界：PhoneCard.vue 由卡片 Agent；AutomationCenter.vue 由集成负责人；新 composable 由运行 Agent。
- 开始条件：T37、T38、T23。

## 实施清单
1. 当 action lease 属于 prepare/script，继续观看原主视频，禁止网页触点/手动 key/text/unlock/群控动作并释放未完成手势。
2. 仅在后端权威确认 holder 释放和清理完成后恢复手动操作；失联/状态未知保持不可写。
3. 保证浮窗开关/全屏变换不驱动新 WebRTC 连接或改变写 holder。

## 验收条件
- [ ] 真实脚本执行期间原视频连续且可见，手控入口实际被 Rust/Executor 拒绝。
- [ ] 执行失败/未知/断网/绑定变更后的只读 UI 状态与后端一致。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
UI disabled 只是提示，后端/Executor 校验不能省略。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
