# T23 · 全屏浮窗宿主与进出全屏位置映射

> 状态：**全屏承载与位置映射代码已提交，待联调验证**；阶段：P08；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片/浮窗 Agent**。
- 文件边界：PhoneCard.vue；新 automation/useAutomationFloatingLayer.ts / AutomationFloatingHost.vue。
- 依赖/领取条件：T21。

## 具体开发步骤
1. 将主浮窗和后续子浮窗挂载到当前 phone-screen fullscreen element 的顶层子树或受控 Teleport 宿主。
2. 全屏进入/退出时保持同一个视频、脚本选择与窗口展开状态；只重新约束坐标，不能在 body 下被遮挡。
3. 处理屏幕旋转、浏览器缩放、窗口大小变化、小屏安全区域及全屏 video 旧 CSS 层级。

## 完成判定
- [x] 设计与实现普通/全屏两套独立坐标，进入/退出时切换位置与边界约束，保持同一浮窗组件和原视频不重建。**3 次真实切换未测。**
- [x] 将主浮窗与未来 T24 子窗挂载宿主保留在 `phone-screen` 内；主窗 z-index 9、子窗 13、展开菜单 12；短横屏自动改侧边紧凑滚动菜单。**未浏览器验证。**
- [x] T24 可复用 `getAutomationSubwindowHost()` 作为 Teleport 目标，不得挂 `document.body`；T23 本轮代码已提交，真实验收仍待 FF-06/FF-07。

## 禁止与风险
不能为了展示编辑 UI 主动调用 exitFullscreen；不得添加第二个 video。

## 交接/验证状态
- 已改 `PhoneCard.vue`、`AutomationFloatingPanel.vue`、`useAutomationFloatingLayer.ts`，未改会话 API、脚本管理或执行器。
- **未执行** TS、lint、构建、单测、手动 CI、浏览器/Android 实机操作，调试模式；待测全屏、旋转、窗口尺寸变化与极短横屏菜单。
