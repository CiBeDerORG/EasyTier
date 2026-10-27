# T23 · 全屏浮窗宿主与进出全屏位置映射

> 状态：**待开发**；阶段：P08；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片/浮窗 Agent**。
- 文件边界：PhoneCard.vue；新 automation/useAutomationFloatingLayer.ts / AutomationFloatingHost.vue。
- 依赖/领取条件：T21。

## 具体开发步骤
1. 将主浮窗和后续子浮窗挂载到当前 phone-screen fullscreen element 的顶层子树或受控 Teleport 宿主。
2. 全屏进入/退出时保持同一个视频、脚本选择与窗口展开状态；只重新约束坐标，不能在 body 下被遮挡。
3. 处理屏幕旋转、浏览器缩放、窗口大小变化、小屏安全区域及全屏 video 旧 CSS 层级。

## 完成判定
- [ ] 进出全屏 3 次后主浮窗位置可见、无视频重连、拖动/滚动/输入正常。
- [ ] 全屏菜单与浮窗不会被视频背景、dismiss 遮罩覆盖。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
不能为了展示编辑 UI 主动调用 exitFullscreen；不得添加第二个 video。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
