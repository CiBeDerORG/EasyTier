# T40 · 一步 ▶ 直接自动准备并执行当前手机

> 状态：**待开发**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
