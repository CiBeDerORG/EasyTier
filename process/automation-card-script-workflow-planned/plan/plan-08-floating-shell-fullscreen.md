# P08 · 悬浮按钮、主子浮窗和全屏基础

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
将脚本交互承载从卡片 footer 的固定布局迁移到视频附近独立可拖动浮窗体系，全部能在 fullscreen top layer 正常交互。

## 前置条件与关联任务
- 前置：P07。
- 开发任务：[T21](../task/task-21-floating-host.md)、[T22](../task/task-22-orb-layout.md)、[T23](../task/task-23-fullscreen-portal.md)、[T24](../task/task-24-child-floating-window.md)。

## 交付要求
1. 复用 PhoneCard.vue 现有主 orb 和拖动锚点；side-actions 的自动化 icon 占原唤醒位置，唤醒上移一格，原功能不变。
2. 主浮窗可从自动化按钮开关；菜单 6s 自动收起不关闭主浮窗，关闭浮窗不关闭视频、不删草稿、不停脚本。
3. 主浮窗标题可拖，小屏边界约束、内容内部滚动，文本点击/拖动不会透传为 Android 手机触控。
4. 一套可重用子浮窗承载编辑/克隆命名/条件删除确认/参数输入，不复用附加到 body 后全屏不可见的 modal。
5. 非全屏和进入/退出全屏转换保留同一个 UI 状态与当前 MediaStream，会话不得重连。

## 可验收判据
- [ ] 普通/全屏能打开主子浮窗并拖动、输入、关闭、滚动，不会跳出全屏或意外触发手机触控。
- [ ] 自动化与唤醒按钮排列准确；其它关闭/录制/导航/解锁按钮行为不变。

## 边界与风险
目前 Fullscreen API 只将 phone-screen 作为 top layer；子窗必须在其中挂载或受控 teleport，不可简单 z-index 提升 body；浮窗主业务状态不能挂在会被全屏重挂载销毁的临时节点。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
