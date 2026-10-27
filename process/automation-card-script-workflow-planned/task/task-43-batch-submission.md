# T43 · 浮窗全局运行入口与不可变任务提交

> 状态：**待开发**｜计划：P14｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**运行 Agent + 集成负责人串行接线**。
- 文件边界：新 automation/useFloatingBatchRun.ts、AutomationScriptPanel.vue；AutomationCenter.vue 由集成负责人。
- 开始条件：T28、T39。

## 实现步骤
1. 主浮窗提供“运行全流程”，目标只来自工作台勾选且授权的手机；0 台明确阻止。
2. 获取当前脚本正式 revision 或已确认草稿 snapshot，冻结步骤/参数名/设备列表/fingerprint/request UUID。
3. 复用现有 ScriptLibrary.prepareRun 与 runs API，避免额外 run dialog 另开预览；不能把单步执行绑定到勾选全集。

## 通过标准
- [ ] 只运行所选手机，连续点按钮或超时不产生重复运行记录。
- [ ] 0 步或未授权脚本禁止执行，后续编辑不修改在途快照。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
先接上 T39 安全写锁移交，不能仅隐藏旧试跑 modal 来绕开排他。

## 本任务当前状态
只建立任务计划；未修改功能代码，未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。
