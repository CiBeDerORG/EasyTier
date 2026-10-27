# T19 · 基线审计与旧入口替代清单

> 状态：**待开发**；阶段：P07；难度：S；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**架构/产品 Agent**。
- 文件边界：文档：require-07、task/README；不改业务代码。
- 依赖/领取条件：T01～T18 现状、FINAL V3。

## 具体开发步骤
1. 列清卡片视频、card timeline、ScriptLibrary、Inspector、CardTrialPanel、PhoneAutomationPanel 等旧功能入口和上游/下游事件。
2. 按“保留内核/迁移 UI/可替换后删”逐项建立删除门槛，指向功能对应的 FF/AC 验收 ID。
3. 核对当前 feat/automation-center HEAD 与已合入的 R0 去重，避免把旧完成状态标为新需求已做。

## 完成判定
- [ ] 形成每个旧入口→新浮窗替代入口→验收标准对应表。
- [ ] 未发现功能无替代就计划直接删除。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
不执行重构/测试，不假设当前源码没有被其他 Agent 修改。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
