# P15 · 旧入口替代后的代码瘦身与兼容清理

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
前端收敛单一脚本逻辑/单一原视频/单一运行结果呈现，删除已被浮窗替代的 UI 壳、重复播放器与路由事件，不丢旧能力。

## 前置条件与关联任务
- 前置：P08～P14 对应能力可用后逐个执行。
- 开发任务：[T46](../task/task-46-obsolete-ui-removal.md)、[T47](../task/task-47-dead-code-cleanup.md)。

## 交付要求
1. 移除已替代的 PhoneCard footer 固定时间线、旧内嵌 CardStepEditor、UiInspectorDialog 重复视频、ScriptLibrary 的独立运行预览与冗余模态试跑 UI。
2. 复用保留 ScriptStepEditor、InspectorPropertiesPanel、PhoneAutomationPanel 的真实控件能力、RunHistory 和准备任务进度；不因删除 UI 删共享逻辑。
3. 删多余 props/emits/import/css、失效的 Tab 跳转，缩减 AutomationCenter 巨型组件的 UI 编排耦合；保留脚本库老链接的安全兼容。
4. 更新回归测试和内部文档，确认历史运行/跨设备恢复/正常手控与自动化的共存。

## 可验收判据
- [ ] 没有两个同时可操作的脚本编辑源、没有第二个自动化视频入口；旧入口迁移后仍能找到全部功能。
- [ ] 重复事件/样式没有悬空引用，现有手控/群控/录制流程不回归。

## 边界与风险
严格按功能替换后删除；若 R4 安全交接未完成，不得预先删除现有 handoff 与异步回执逻辑。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
