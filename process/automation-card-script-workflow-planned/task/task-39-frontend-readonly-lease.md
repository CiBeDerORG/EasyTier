# T39 · 前端只读观看与执行态输入屏蔽

> 状态：**状态：单步试跑及原卡片Viewer接线已提交，旧批量路径待迁移、真机待验收**｜计划：P12｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
计划阶段描述已由下方实际交付记录补充；代码已提交、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T39 实际代码交付与保留边界
- `usePhoneSession.ts` 原 WebRTC `stream`、主 `video`、`sessionId` 不换；请求降权前本地拒写并撤销排队指令（不重复发送未知动作），待后端确认才允许继续进入脚本。恢复手控需 Rust 确认后再放开。迟到的旧控制回执不会误关闭媒体。
- `PhoneCard.vue` 观看期间和 `run_occupied` 状态下禁 tap/key/text/unlock/群控及未完触摸，保留可见视频和浮窗；增加显式“恢复手控”按钮，单步试跑对话框未结束时不得提前恢复。
- `CardTrialPanel.vue` 单步骤原卡片试跑不再调用旧 `releaseOwnedTrialPreviews` 强制关闭本机视频，改为仅对本卡片拥有的确切会话降为 `view_only`，重新读取服务端 `active_session.id/state`、授权、指纹，在准备和 POST/run 前两次核验；原来的请求 ID/执行历史未知保护不删。
- **保留边界**：旧 `ScriptLibrary` 与批量试跑流程仍可能使用 `releaseOwnedTrialPreviews`，尚未统一到同屏只读移交；`CardTrialPanel` 仍通过 body Dialog，进入单步试跑曾有退出全屏的旧处理，需 T40 修复。旧镜像必须拒绝不支持的降权请求，绝不无条件跳过清理。
- **待验证**：真实全屏及普通模式视频不断流、两浏览器、试跑失败/未知状态、脚本完成恢复手控、坐标/UiAutomator2、状态过期；**未执行**任何本地/远程测试或构建（调试模式）。
