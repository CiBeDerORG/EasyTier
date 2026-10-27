# T20 · 统一设备绑定、草稿、浮窗及运行事件契约

> 状态：**公共契约已交付（待验证）**；阶段：P07；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**集成负责人**。
- 文件边界：easytier-web/frontend/src/components/AutomationCenter.vue；types/automation.ts；新增契约文档。
- 依赖/领取条件：T19。

## 具体开发步骤
1. 冻结 ownerId、deviceId、sessionId、targetFingerprint、stepId、scriptId/draftId、revision、runId、requestId 的来源与失效规则。
2. 定义一份 owner 共享草稿与 per-card 独立浮窗可见性/观看/执行状态，保证点击单步目标来自当前卡片。
3. 冻结 editor dirty、CAS 冲突、切脚本保护、fullscreenchange 的 API/events；约定各 Agent 不直接修改集成文件。

## 完成判定
- [x] 交付逐事件字段、状态转移及跨手机/账号拒绝规则；见 `../require/require-09-t20-workspace-events-contract.md`。
- [x] 公共类型/纯函数不持有视频、脚本或 run；只复用 `AutomationCenter` 的已有共享工作区。
- [x] 交接公共 TS 契约与 scope 守卫，列出后续 T21/T25/T36 的联调边界；**代码尚未测试**。

## 禁止与风险
禁止在多个组件中各建一套脚本同步状态；此任务由集成负责人独占接线。

## 交接/验证状态
- 已创建 `automation-floating-contract.ts`（共享类型/快照/校验纯函数）；`AutomationCenter.vue::selectCardStep` 使用统一 owner/device/workspace 校验。
- 浮窗渲染/视频运行/跨设备数据迁移均待后续任务；本次未测试/构建，调试模式。
