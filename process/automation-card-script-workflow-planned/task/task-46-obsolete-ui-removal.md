# T46 · 旧双脚本编辑与重复视频展示层清理

> 状态：**待开发**｜计划：P15｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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
只建立任务计划；未修改功能代码，未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。
