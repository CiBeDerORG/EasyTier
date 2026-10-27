# T21 · 可拖拽自动化主浮窗容器

> 状态：**浮窗组件已提交，等待 T22 入口和 T23 全屏联调**；阶段：P08；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片/浮窗 Agent**。
- 文件边界：新建 automation/AutomationFloatingPanel.vue 和 useAutomationFloatingLayer.ts；PhoneCard 由卡片 Agent 唯一接线。
- 依赖/领取条件：T20。

## 具体开发步骤
1. 创建仅展示/拖动 UI 的主浮窗承载，标题栏为唯一 drag handle，内容内部滚动。
2. 浮窗打开/关闭不更改 video.srcObject、WebRTC session、执行和草稿身份；独立于 orb menuOpen 与菜单定时收起。
3. 将浮窗位置限制在可见区域，支持触摸和鼠标 pointer capture/cancel，阻止拖动误发手机 touch。

## 完成判定
- [x] 实现受控 visible/close、标题栏拖动、内容滚动及 pointer 事件隔离；**未浏览器验证**。
- [x] 仅本机 UI open 状态，关闭事件不涉及视频/草稿/运行逻辑；**未实机验证**。
- [x] 输出 `AutomationFloatingPanel.vue` + `useAutomationFloatingLayer.ts` + `PhoneCard.vue` 受控挂载/开关 API；T22 负责可见按钮接入，T23 负责 fullscreen 联调。

## 禁止与风险
先实现容器、使用占位内容；此任务不搬脚本业务或改运行权限。

## 交接/验证状态
- 已开发 UI 承载壳与独立控制状态，当前卡片原时间线仍保留；用户在 T22 完成前没有图标入口。
- T21 的手动开关通过 PhoneCard expose 提供给后续集成，**未执行 TypeScript/浏览器/真机验证**（调试模式）。
