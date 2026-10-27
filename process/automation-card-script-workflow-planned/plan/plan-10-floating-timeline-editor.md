# P10 · 悬浮步骤列表、全部删除与统一编辑

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
将旧卡片 footer 时间线迁入主悬浮窗，所有步骤有删除入口，参数编辑、重拾/演示在子浮窗完成。

## 前置条件与关联任务
- 前置：P08 + P09。
- 开发任务：[T29](../task/task-29-timeline-migration.md)、[T30](../task/task-30-ordinary-step-delete.md)、[T31](../task/task-31-branch-delete-undo.md)、[T32](../task/task-32-floating-step-editor.md)。

## 交付要求
1. 移动 CardStepTimeline 的单实例到主浮窗，退出旧 footer 位置，保留序号/条件层级/▶/✎/步数状态。
2. 普通步骤的 🗑 删除 + 短时撤销；删除从 owner 共享草稿保存 CAS，不改变原脚本正式修订或 run 历史。
3. 条件节点/分支子步骤同样有 🗑；条件块联动删除先显示影响范围，分支末项禁止单删时提供安全删除所属条件块的路径。
4. 共用 ScriptStepEditor 表单与既有 updateCardDraftStep 防冲突校验，CardStepEditor 外壳变可拖动子窗；保留重拾、只读定位演示和参数保存。

## 可验收判据
- [ ] 每一行，包括 if_else/if_gone/分支子步骤，均能找到删除入口，删除后 x/100 与序号同步；旧 stepId 保持。
- [ ] 只有一份正在编辑的共享步骤内容，A/B 浮窗并存时不得冲突覆盖；未保存修改可取消切换。

## 边界与风险
先完成浮窗替代，最后才从 PhoneCard 移除内嵌 timeline/editor；复用 automation-flow-tree.removeFlowBlock，不能直接 splice 条件数组。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
