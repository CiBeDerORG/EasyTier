# T40 · 一步 ▶ 直接自动准备并执行当前手机

> 状态：**代码主体已提交，安全验证与体验闭环待补齐**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**运行 Agent**。
- 文件边界：新 automation/useInlineStepRun.ts；useCardTrialPreparation.ts 可复用；CardTrialPanel.vue 暂保留兼容。
- 开始条件：T32、T39、T34。

## 实施清单
1. 主浮窗中点击某一步 ▶ 锁定当前卡片 deviceId/ownerId、stepId/当前 steps 快照及 UUID，不弹常规试跑确认。
2. 检查 ADB、UIAutomator2、绑定、唯一 selector、快照新鲜度、坐标尺寸；必要组件初始化自动准备。
3. 参数缺失或需敏感操作确认时显示同画面可拖子浮窗；硬不兼容直接阻断；条件节点单步按钮禁用。
4. 只提交一次真实动作；网络未知只读查询原请求，用户主动重新试跑才产生新 UUID。

## 验收条件
- [ ] 点 A 卡片 2 步只执行 A 手机 2 步；自动准备过程中画面不中断。
- [ ] 没有普通“手机匹配风险”重复勾选，仍拒绝不兼容或未授权步骤。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
不能简单删除 CardTrialPanel.prepare 的 preview close 逻辑，不完成 lease 协议不得启用这一改动。

## 验证状态
相关代码已提交，但尚未测试或构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T40 代码交付（调试模式）

- `modules/useInlineStepRun.ts` 新增点击时冻结 ownerId、deviceId、target fingerprint、stepId、不可变 JSON 快照与一次性 request UUID；后续 POST/run 使用该请求 UUID 而不是准备完重新生成，目标/步骤变化 fail-closed。
- `PhoneCard.vue` 取消“打开单步试跑前退出全屏”；`CardTrialPanel.vue` 已从 appendTo body 的 PrimeVue modal 改成挂载原 `PhoneCard.automationSubwindowHost` 的 `AutomationFloatingDialog` 可拖动子浮窗；全屏和普通模式共享原视频/同会话，不创建第二条流。
- 点击步骤 ▶ 自动触发准备与既有 ADB/UiAutomator2 检查，使用 T39 服务端确认的同 sessionId viewer 移交；准备成功后对 `read_only + compatible + 无参数` 的只读检查自动提交一次；涉及真实动作、截图/定位风险或动态输入，合并成同子浮窗一次明确确认再发送动作。
- 提交前继续验证 `active_session.state=view_only` 与会话/绑定指纹、兼容性快照 20 秒时效；网络未知保留旧 sessionStorage 请求回执，仅只读查询，不自动重复 POST。同卡片的另一 ▶ 在当前浮窗未关闭前被禁用。
- 未完成：更精细的高风险/普通操作策略（当前所有实际输入动作保留确认）；UiAutomator2 严格匹配已完成源码接线，但覆盖率与真机验证仍待 T42/QA 补充；错误/确认子窗实际排版及移动端多指手势需浏览器验收。
- **无测试、构建、手动 CI、浏览器或 FnOS/Android 真机验收**；所有现有验收框不勾选。

- 后续安全加固：`reviewAutomationDeviceCompatibility` 增加可选 `requireUniqueSelectors`（仅单步真实执行启用）。控件树截断、XPath 无法只读证明唯一、候选为零或多于一个时从“人工确认”升级为硬阻断；准备、提交前再核验与异常只读复核三个入口一致，错误文案不再只提示“尺寸不符”。提交 `8a10b2f7`、`b099bfc0`、`18030ab6`；该安全路径仍需 Browser+Android 真机验证。
