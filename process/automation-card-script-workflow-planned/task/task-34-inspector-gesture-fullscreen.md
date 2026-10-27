# T34 · 控件树拾取与拖拽步骤全屏联调

> 状态：**待开发**｜计划：P11｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Inspector Agent**。
- 文件边界：UiInspectorDialog.vue、InspectorPropertiesPanel.vue；坐标纯函数；PhoneCard 由卡片 Agent 串行接线。
- 开始条件：T33、T23。

## 实施清单
1. 在主视频中原位右键（触摸采用现有替代）选控件、滑动/拖拽起点终点；浮窗配置动作并录入。
2. 确认每步后保留拾取模式，连续添加 1～4 步；重新拾取只替换原 stepId。
3. 测试协议需要覆盖屏幕旋转、视频 contain 黑边与虚化全屏背景、全屏浮窗拖动阻止视频误触和旧控件树失效。

## 验收条件
- [ ] 四步录入、保存和重新拾取不重启投屏、不切 Tab，不误对真实手机执行演示。
- [ ] 高亮矩形命中在普通和全屏下准确，装饰 ambientVideo 不接收事件。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
不能把全屏出现的装饰 ambientVideo 当作另一个操作视频；只有主视频可参与定位与动作。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
