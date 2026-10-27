# T19 · 基线审计与旧入口替代清单

> 状态：**审计交付已完成，未测试**；阶段：P07；难度：S；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**架构/产品 Agent**。
- 文件边界：文档：require-07、task/README；不改业务代码。
- 依赖/领取条件：T01～T18 现状、FINAL V3。

## 具体开发步骤
1. 列清卡片视频、card timeline、ScriptLibrary、Inspector、CardTrialPanel、PhoneAutomationPanel 等旧功能入口和上游/下游事件。
2. 按“保留内核/迁移 UI/可替换后删”逐项建立删除门槛，指向功能对应的 FF/AC 验收 ID。
3. 核对当前 feat/automation-center HEAD 与已合入的 R0 去重，避免把旧完成状态标为新需求已做。

## 完成判定
- [x] 形成每个旧入口→新浮窗替代入口→验收标准对应表。
- [x] 所有旧入口已明确替代后删除门槛，不安排无替代直接删除。
- [x] 交付 T20 所需状态字段、设备/脚本生命周期风险及失效场景；详见 `../require/require-08-t19-baseline-audit.md`。

## 禁止与风险
不执行重构/测试，不假设当前源码没有被其他 Agent 修改。

## 交接/验证状态
- T19 已完成代码审计与替代矩阵；仅文档，无测试/构建/真机，调试模式。
- 审计基线：`91392fd26e001071a1d4d77b4edb5f96e9bdfbb5`。
