# T20 · 统一设备绑定、草稿、浮窗及运行事件契约

> 状态：**待开发**；阶段：P07；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**集成负责人**。
- 文件边界：easytier-web/frontend/src/components/AutomationCenter.vue；types/automation.ts；新增契约文档。
- 依赖/领取条件：T19。

## 具体开发步骤
1. 冻结 ownerId、deviceId、sessionId、targetFingerprint、stepId、scriptId/draftId、revision、runId、requestId 的来源与失效规则。
2. 定义一份 owner 共享草稿与 per-card 独立浮窗可见性/观看/执行状态，保证点击单步目标来自当前卡片。
3. 冻结 editor dirty、CAS 冲突、切脚本保护、fullscreenchange 的 API/events；约定各 Agent 不直接修改集成文件。

## 完成判定
- [ ] 交付逐事件字段、状态转移和跨手机/账号切换的拒绝规则。
- [ ] 未引入第二套共享草稿数据/视频控制器。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
禁止在多个组件中各建一套脚本同步状态；此任务由集成负责人独占接线。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
