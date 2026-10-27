# P11 · 原视频控件拾取、属性与动作录入浮窗

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
把 Inspector 的控件树/属性/定位候选与动作配置移到可拖动子浮窗，原卡片视频始终是唯一拾取/高亮画面。

## 前置条件与关联任务
- 前置：P08；P07 合同。
- 开发任务：[T33](../task/task-33-inspector-main-video.md)、[T34](../task/task-34-inspector-gesture-fullscreen.md)、[T35](../task/task-35-inspector-legacy-exit.md)。

## 交付要求
1. 使用 PhoneCard 主 video 的有效像素变换和 CardPickingOverlay/只读高亮，不在 Inspector 内再渲染第二视频。
2. UiAutomator2 树、rankLocatorCandidates、XPath/坐标/拖拽步骤等仍复用现有业务逻辑；不以删预览删除功能。
3. 确认录入后收起配置子窗并保持拾取模式，连续录入 1～4 步；编辑已存在步骤只替换对应 ID/定位，不新增重复步骤。
4. 全屏/旋转/分辨率变化/旧快照校验严谨，退出全屏后弹窗仍可重新打开，不切脚本库 Tab。

## 可验收判据
- [ ] 一个设备一个原会话、一个主操作 video；不出现 UiInspector 的小视频。
- [ ] 树属性、候选、高亮、真实像素命中、拖拽起终点在普通和全屏模式可用。

## 边界与风险
PhoneCard 还有一个 ambientVideo 用于全屏背景虚化，属于当前同一 stream 的装饰层，不可误当成“另一个操作画面”并破坏全屏策略；重构应保证真正操作视频只有原主 video。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
