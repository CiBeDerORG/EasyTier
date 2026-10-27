# T34 · 控件树拾取与拖拽步骤全屏联调

> 状态：**业务代码已提交，待真实验证**｜计划：P11｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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

## 2026-10-27 · 代码交付与验证状态
- 继续在原 primary-video 右键拾取，原屏 selected node 的 bounds 按同一 object-fit:contain 的 native 坐标画 SVG 高亮；拖拽 A/B 进入临时只读覆盖模式，捕获 pointer 后只回给 Inspector，不发 ADB touch。
- 画面尺寸/方向变化使旧树、坐标确认及拖拽点位失效，重新抓取控件树；提交拖拽前要求树快照与主视频真实像素尺寸一致；多步录入和重新拾取仍经过旧 picker generation/binding/stepId 审查。
- **尚未验证**：普通/全屏反复切换、视频旋转/黑边、触屏拖拽 A/B 两点、四步连续录入、重新拾取不换 stepId、控件树截断/过期/唯一性以及跨设备/双浏览器异步回执。
- **调试模式**：本轮未运行 TS、lint、单测、构建、手动 CI、浏览器或真机测试；源代码提交不等于全部验收条件已通过。
