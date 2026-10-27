# T33 · Inspector 属性树基于原主画面浮窗

> 状态：**待开发**｜计划：P11｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Inspector Agent**。
- 文件边界：UiInspectorDialog.vue、InspectorPropertiesPanel.vue；CardPickingOverlay.vue 由卡片 Agent 接线。
- 开始条件：T23、T20。

## 实施清单
1. 移除 Inspector 必须自带预览的交互前提；属性树/候选/动作配置渲染于浮动子窗。
2. 命中及位置来自 PhoneCard 原主 video 像素映射；控件树快照可独立刷新，树节点选择高亮原卡片主视频。
3. 保留 UiAutomator2 截断、过期、唯一匹配、跨设备 fingerprint 检验；只读检查不自动发真实动作。

## 验收条件
- [ ] 打开 Inspector 不出现第二个操作视频；即使浮动面板移开，原视频仍可见。
- [ ] 属性树、XPath、resource id、文本候选和可点击父节点表现不回退。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
不能仅删 UiInspectorDialog video 标签而丢失拖拽起终点等依赖视觉坐标的行为；先提供等价通道。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
