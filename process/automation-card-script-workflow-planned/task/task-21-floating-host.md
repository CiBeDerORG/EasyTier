# T21 · 可拖拽自动化主浮窗容器

> 状态：**待开发**；阶段：P08；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片/浮窗 Agent**。
- 文件边界：新建 automation/AutomationFloatingPanel.vue 和 useAutomationFloatingLayer.ts；PhoneCard 由卡片 Agent 唯一接线。
- 依赖/领取条件：T20。

## 具体开发步骤
1. 创建仅展示/拖动 UI 的主浮窗承载，标题栏为唯一 drag handle，内容内部滚动。
2. 浮窗打开/关闭不更改 video.srcObject、WebRTC session、执行和草稿身份；独立于 orb menuOpen 与菜单定时收起。
3. 将浮窗位置限制在可见区域，支持触摸和鼠标 pointer capture/cancel，阻止拖动误发手机 touch。

## 完成判定
- [ ] 主浮窗可打开/关闭/拖动/滚动，手机触控流在拖动过程中无事件。
- [ ] 录制/投屏/脚本运行在关闭浮窗后继续保持原状态。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
先实现容器、使用占位内容；此任务不搬脚本业务或改运行权限。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
