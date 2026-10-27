# T42 · 单步失败修复与旧试跑弹窗退休

> 状态：**回修入口已迁移、保存后人工重试已接线；待联调验收**｜计划：P13｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
回修代码已提交，尚未测试或构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
## 2026-10-28 · T42 首批实际代码

- `PhoneCard.vue` 原底部 `CardRepairEntry` 迁移到原手机 `automationSubwindowHost` 内的 `AutomationFloatingDialog`，可拖、可全屏显示，无第二播放器；仍定位原 runId、stepId，并经父 `AutomationCenter.onCardTrialRepair` 读取最新持久 run 快照及目标。
- 结果 `outcome_unknown` 时浮窗顶部关闭入口不可绕开人工确认，保留 `CardRepairEntry` 原独立确认后才能关闭/重试的保护。
- `CardRepairEntry` 再试跑要求现有步骤与历史失败快照不同，且前端脚本/草稿同步 guard 为 `clean`；修改未保存/有冲突不能产生新动作。再次试跑必须用户主动点击，使用 T40 全新 intent UUID，而不是修改历史 run。
- 原 `CardTrialPanel` 已改造为原视频可拖子浮窗，旧 body modal 渲染路径已撤销；原安全检查、历史导航、异常回执恢复逻辑继续复用，不提前删除底层保护。
- **尚需验收**：Failed/Unknown → 查看历史快照 → 编辑/重拾 → 保存新修订 → 新▶ 的 UI/服务端闭环、双浏览器、移动端触摸与浮窗层级；旧 RunHistory 独立页面与跨 Tab 导航仍保留，需 T46 替代完成后再讨论删除。
- **未执行** TS/Rust/Python 检查、单测、构建、GitHub Actions 手动触发、浏览器或 FnOS/Android 真机验证（调试模式）；验收框仍未勾选。
