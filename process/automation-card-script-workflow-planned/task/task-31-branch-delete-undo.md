# T31 · 条件分支及子步骤安全删除、范围确认与撤销

> 状态：**待开发**｜计划：P10｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**步骤 Agent**。
- 文件边界：CardStepTimeline.vue + automation-flow-tree.ts 的复用适配；新 AutomationConditionDeleteDialog.vue。
- 开始条件：T29、T30、T24。

## 实施清单
1. 条件节点和分支中每一条子步骤均保留可见 🗑。
2. 删除条件根节点前计算后续 Then/Else 包含的受影响步骤并显示可拖动确认子窗；确认后用 removeFlowBlock。
3. 删除分支最后一步若破坏结构，给出删除所属条件块等明确安全方案，不直接 splice；保留撤销与 CAS 冲突保护。

## 验收条件
- [ ] 普通分支子步骤优先只删除当前步骤；条件块确认后才连带删除。
- [ ] 不会出现缺失 Then/Else 的非法脚本，也不会影响已开始的 run 快照。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
不可用“条件不能删除”为由隐藏删除按钮；旧脚本实际修订不自动覆盖。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
