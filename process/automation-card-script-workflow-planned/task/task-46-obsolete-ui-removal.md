# T46 · 旧双脚本编辑与重复视频展示层清理

> 状态：**旧双重编辑/试跑界面代码已退役，兼容目录只读；自动验收中、真机待测**｜计划：P15｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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

## 2026-10-28 · T46 第二批代码交付：正式脚本管理与条件编辑迁入浮窗

- **发布/修订 CAS**：`automation-script-workspace.ts` 直接复用唯一 owner-level `draftWorkspace/draftSync`，禁止 dirty editor、未同步草稿或版本冲突状态下正式发布。已保存脚本按 revision 进行服务端 CAS 更新，尚未创建正式脚本 ID 时可从当前共享草稿发布新脚本，均不会暗中修改历史运行快照。
- **正式删除 CAS**：Rust `db/automation/scripts.rs` 增加事务内带修订号的 `delete_script_if_revision`，REST `POST /scripts/{id}/safe-delete`，前端 API `deleteScriptVersioned`。旧无修订 DELETE 保留兼容但第一方新浮窗及 ScriptLibrary 删除入口改用新 CAS；不允许跨浏览器旧修订删除新数据。删除当前正式脚本后保留草稿步骤并解除旧正式 revision 指向。
- **可移动子窗**：新 `AutomationScriptManageDialog.vue` 通过原 PhoneCard `automationSubwindowHost` 呈现发布说明、脚本名称、确认删除、修订来源与错误；`AutomationScriptSelector` 提供发布、逐脚本删除和只读核验；`PhoneCard` 转发给 `AutomationCenter` 唯一共享 ScriptWorkspace。未新增视频、编辑器或命令执行器。
- **未知正式写入**：正式发布/删除请求前在 sessionStorage 只保存 owner、scriptId/revision、期望脚本名/stepIds 与类型，不保存定位器、明文、脚本参数。HTTP 不确定时禁止重复写操作；只读核对当前服务端修订及步骤 ID，无法确定原结果则继续阻断。旧 trial UUID 的限制仍然存在。
- **回修去重**：`RunHistory.vue` 两个失败修复按钮合为“回原手机卡片定位并修复”，仅发 `repair-card`；`AutomationCenter.vue` 调用已有 `onRepairCard` 身份/原历史快照/原修订核验，旧 `repairRequest` 跨 Tab 链删除；`ScriptLibrary.vue` 旧历史临时复制/第二编辑分支删除。
- **条件编辑迁移**：`CardStepTimeline` 的新增动作、普通步骤→ If/Else 条件、Then/Else 分支新增、同分支步骤上下移均派发到父层唯一共享草稿；沿用 `analyzeAutomationFlow`、`createFlowCondition`、`addFlowBranchStep` 与每步 ID 保留规则，新增占位步骤未配置前不可执行。 `CardStepEditor` 增加末尾无条件断言的业务成功验证勾选，保存时后端前 UI 二次拦截其它标记与错误结构。
- **保留兼容业务**：旧 ScriptLibrary 独立编辑区目前仍可打开作历史用户兼容（包含条件图与现有正式 CRUD），原新浮窗已接齐核心编辑接口。因为尚无浏览器/多设备验收，**不强行删除旧脚本管理 Tab**。T47 dead-code cleanup 应区分真正无引用的旧 helper 与仍服务于老运行历史恢复的基础逻辑。
- **未执行** Vue/TypeScript/Rust 编译、类型检查、lint、单测、CI/Runner 构建、FnOS/Android、全屏及双浏览器测试（仓库 AGENTS.md 调试模式）；本任务仍待 T49～T51 联调，验收框不勾选。
## 2026-10-28 · T46 第三批：实际移除旧脚本双编辑入口
- `ScriptLibrary.vue` 由原约 46KB 多功能编辑/正式 CRUD/条件步骤页面收敛为约 8KB 的**只读兼容目录**：只保留脚本列表、原手机跳转、旧试跑未知 UUID 的服务端历史只读核对。移除旧 ScriptStepEditor 引用、该页的正式保存/删除、条件编辑和所有旧步骤事件。
- `AutomationCenter.vue` 的兼容选中脚本只能调用已有 `PhoneCard.openAutomationPanel` 和单份 owner-level `scriptWorkspace.perform({kind:'select'})`，保留修订 CAS、同步草稿阻断和设备授权。已删除整个不再存在调用方的 `openScriptInspector` 及旧 `pick-step-target` 接线。
- 功能没有“删无替代”：新卡片提供脚本选择/克隆/正式发布/删除/条件增删编辑、运行与修复浮窗；旧历史请求继续在兼容页只读核对。共享 `ScriptStepEditor` 仍被 `CardStepEditor` 真正使用，`UiInspectorDialog` 仍由 `PhoneCard` 使用，故不误删。
- 新增 `automation-legacy-retirement-t48.test.mjs` 防回退断言，阻止旧页再次创建独立保存/删除/运行/视频/第二编辑器；加入 `test:unit`。**代码退役完成，仍需前端 CI/无手机浏览器及最终 T50 实测证实业务行为。**
