# T24 · 统一可拖动子窗、焦点与关闭保护

> 状态：**子浮窗公共组件已提交；待业务接线与浏览器验证**；阶段：P08；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**浮窗 Agent**。
- 文件边界：新 automation/AutomationFloatingDialog.vue、useAutomationFloatingLayer.ts（与 T21/T23 同一 Agent 串行）。
- 依赖/领取条件：T23。

## 具体开发步骤
1. 提供子窗统一壳供克隆重命名、步骤编辑、条件删除确认、参数输入/结果查看。
2. 明确只对必要操作显式确认，Esc 仅关闭顶部可关闭的子窗，关闭不隐式提交/取消正在执行动作。
3. 支持内容滚动/焦点返回/键盘/拖动误触隔离，全屏挂入同一安全宿主。

## 完成判定
- [ ] 主窗、子窗支持位移和边界约束的代码已实现，**待普通/全屏真实浏览器验证**。
- [ ] 子窗已实现 dirty 关闭保护界面，实际草稿编辑/保存接线须等待 T32；真实未保存回归仍未做。
- [x] 已交付组件 API、host 约束、Escape/焦点/脏编辑契约；后续集成方必须处理 `@close="reason => ..."` 的 discard 数据清理。

## 禁止与风险
本任务不重新实现 ScriptStepEditor 或脚本数据，子窗只作承载层。

## 交接/验证状态
- 新增 `AutomationFloatingDialog.vue` 与 `automation-floating-dialog-stack.ts`，扩展 `useAutomationFloatingLayer.ts` 支持子窗独立初始位置/键盘移动；不修改 Rust、Executor 或 WebRTC。
- **子窗仅为公共基础设施**，尚未替换步骤编辑、克隆和删除条件窗口，不宣称 UI 业务闭环。
- **未执行**单测、TS 检查、lint、构建、手动 CI、浏览器/安卓真机（当前调试模式）。
