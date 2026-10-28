# P15 · 旧入口替代后的代码瘦身与兼容清理

> **状态：T46首批重复试跑/副视频已删除，历史管理兼容和T47待处理**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

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
T46已有实际代码提交；缺乏运行验收证据时仍按待联调处理，不把文件创建计作交付。
## 2026-10-28 · P15 阶段执行状态

- **已删**：ScriptLibrary 第二可操作试跑视频/独立试跑 Dialog、重复设备预检/POST 与 AutomationCenter 旧 `prepareRunDevice/releaseRunPreviews`（T40～T45 同画面试跑/批量运行替代）。旧视频 CSS/Run Modal 临时状态随之删除。
- **未删**：正式脚本的保存/删除、旧 Tab 中条件流程编辑和历史修复、旧单步请求 UUID 未知结果只读恢复、RunHistory 独立业务。T46 不以删除 UI 为由弱化幂等或写锁安全。
- 新批量运行入口主动读取旧 trial receipt 并禁止未知动作自动重放；若存在旧请求需去兼容管理页面只读核对，原历史 run 不变。
- **剩余 T46**：浮窗端完整覆盖旧正式脚本 CRUD 和历史回修入口后，再安全关闭兼容脚本编辑页；**T47** 清理已无引用的通用 helper/props 和冗余样式。没有构建/浏览器/真机验证，不标 P15 完成。
