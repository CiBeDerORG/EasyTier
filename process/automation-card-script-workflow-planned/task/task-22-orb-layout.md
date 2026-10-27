# T22 · 悬浮 icon 重新排位与开关

> 状态：**代码已提交，待验证**；阶段：P08；难度：S；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片 Agent**。
- 文件边界：easytier-web/frontend/src/components/automation/PhoneCard.vue（唯一写入）。
- 依赖/领取条件：T21。

## 具体开发步骤
1. 原 side-actions 的唤醒按钮位置改为自动化 icon，唤醒 icon 上移一格。
2. 自动化按钮一击打开浮窗，再击关闭；按钮有明确 aria-label、激活态、键盘支持。
3. 保留原关闭投屏/全屏/录制/返回/最近/主页/手动唤醒操作和现有 dock 左右停靠。

## 完成判定
- [x] 已将自动化图标放在原唤醒所在侧边第三槽；唤醒移至上方操作组并保留原 WAKE 指令（未验证）。
- [x] 自动化按钮不依赖 manualAllowed 禁用；受控切换 T21 浮窗 UI 状态、包含 aria-pressed、i18n tooltip（未验证）。
- [x] T22 交接：`PhoneCard.vue` + `messages.ts`；四个上方按钮新增动态纵向位置约束；T23 负责全屏/短横屏布局及真实浏览器验证。

## 禁止与风险
只改本手机局部菜单；禁止把全局批量脚本执行绑定到这个开关图标。

## 交接/验证状态
- 代码已提交，页面按钮行为待真实验收；未执行本地测试、TypeScript、构建、浏览器或手机操作（调试模式）。
- 注意：极短的横屏视频高度可能无法同时容纳上方四项+下方三项，T23 要验证缩小/收纳交互，不能以“中间停靠”掩盖裁切。
