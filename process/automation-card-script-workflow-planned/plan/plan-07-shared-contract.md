# P07 · 唯一画面与共享状态合同

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
冻结唯一主视频、账号共享脚本、逐机浮窗与运行上下文，统一各组件的 props/events/API 生命周期，作为后续独立 Agent 的交接合同。

## 前置条件与关联任务
- 前置：无（可并行预备后端空脚本调研）。
- 开发任务：[T19](../task/task-19-baseline-audit.md)、[T20](../task/task-20-contract-and-events.md)。

## 交付要求
1. 画面只有 PhoneCard 原实时 MediaStream；自动化不能另建第二视频或为编辑 changeTab。
2. 一个 owner 级脚本草稿与正式脚本 ID/revision；逐设备 video/session/浮窗 UI/运行进度分别存储，不建立每机脚本副本。
3. 确定执行的设备来源：单步=浮窗所属卡片，整套=工作台勾选授权设备，切设备/账号时禁止回调串台。
4. 明确草稿 CAS、未保存编辑、历史快照不可变、run receipt/幂等和视频 viewer/action lease 的合同。

## 可验收判据
- [ ] 所有新增事件显式携带 deviceId/ownerId/stepId 和必要 generation/requestId。
- [ ] 交付结构图、事件表、时序与失效矩阵；旧 T01～T18 的能力不得被隐式替换。

## 边界与风险
AutomationCenter.vue/ScriptLibrary.vue 是共享集成热点，只允许集成负责人接线；不把状态类型写两份。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
