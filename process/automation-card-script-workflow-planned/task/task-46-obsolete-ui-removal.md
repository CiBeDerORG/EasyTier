# T46 · 旧双脚本编辑与重复视频展示层清理

> 状态：**旧重复执行入口已删除，独立脚本管理兼容页待替代验收**｜计划：P15｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**前端集成负责人**。
- 文件边界：AutomationCenter.vue、ScriptLibrary.vue；PhoneCard/Inspector/CardTrialPanel 分别由各组件唯一 Owner 交付去旧 patch。
- 开始条件：T35、T42、T45、T32、T29。

## 实现步骤
1. 逐项对照 R0～R5 取舍表，确认有新浮窗替代后移除 footer 重复时间线与独立 ScriptLibrary 日常自动化工作流入口。
2. 移除 UiInspectorDialog 的第二操作视频及 ScriptLibrary.previewPlayer、重复试跑 modal/跳 Tab 为编辑路径。
3. 保留 ScriptStepEditor、InspectorPropertiesPanel、PhoneAutomationPanel 的组件状态查询及 RunHistory 真正独立业务功能。

## 通过标准
- [ ] 删掉的每一个入口均有可用的主浮窗/子窗功能对应，旧用户任务无遗漏。
- [ ] 不再存在后台自动化逻辑主动走另一个操作视频或独立脚本页。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
删除与对应组件 Agent 的代码改动必须串行，不允许多个 Agent 同时更改 PhoneCard/ScriptLibrary。

## 本任务当前状态
已交付首批重复入口删除，未执行类型检查、构建、CI 或真机验收，详见下方记录。
## 2026-10-28 · T46 第一批代码交付（调试模式）

- `easytier-web/frontend/src/components/automation/ScriptLibrary.vue`：移除旧 `runVisible`、单步/批量提交、独立预检、`previewPlayer` 第二个视频、重复试跑 Dialog、关联临时状态和失效 CSS。单步 ▶ 和全流程 ▶ 的可操作路径分别保留在 `PhoneCard` 原画面内的 `CardTrialPanel` 与 `FloatingBatchRunPanel`。
- `easytier-web/frontend/src/components/AutomationCenter.vue`：删除旧 `scriptPreviewIds`、`releaseRunPreviews`、`prepareRunDevice` 及 `releaseOwnedTrialPreviews` 在此旧路径的依赖和对 ScriptLibrary 的重复参数传入；原手控/WebRTC session 不再因打开旧试跑界面而被该路径自动关闭。
- **保留兼容能力**：正式脚本编辑、保存、删除、条件分支、`ScriptStepEditor`、旧历史修复/只读恢复、`RunHistory`、`UiInspectorDialog`、`PhoneAutomationPanel` 均未删除。旧脚本管理 Tab 仍可进入保存管理，待其完整功能迁入浮窗且经过实机验收后才可下线，避免历史用户丢失删除/修订功能。
- 新 `useFloatingBatchRun.ts` 读取旧 `automation-trial-receipt` 每机遗留 UUID：如原试跑结果未知，拒绝新的 batch POST，提示从兼容脚本管理页只读核对历史记录；删除模态不能绕过旧幂等保护。
- **尚待**：已保存脚本完整编辑/正式保存/删除全迁入浮窗、RunHistory 历史修订入口重定向、UI 新旧功能逐项审查、同页关闭/重开与双浏览器完整回归。不能把本批精简称为 T46 全验收通过。
- **未执行** TypeScript、lint、Rust/Python 单测或编译、构建、CI 手工触发、浏览器/FnOS/Android 真机操作（当前调试模式）。原验收复选框保持未勾选。
