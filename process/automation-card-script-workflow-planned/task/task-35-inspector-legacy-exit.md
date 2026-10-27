# T35 · Inspector 第二视频和跨页编辑引导替换

> 状态：**待开发**｜计划：P11｜复杂度：S-M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Inspector Agent + 集成负责人按文件串行**。
- 文件边界：UiInspectorDialog.vue 由 Inspector Agent；AutomationCenter.vue 由集成负责人；旧 CSS/事件后清理。
- 开始条件：T33、T34、T32。

## 实施清单
1. 将“去编辑脚本”动作从 changeTab('scripts') 调整为打开主浮窗/当前对应步骤子窗。
2. 在新画面拾取、属性、拖拽和独立脚本库无视频检查等替代完备后，移除 Inspector 重复 <video>及其 setVideo/监听/样式。
3. 检查无引用 props.stream、缩放计算不会再暗中读取已删除的小视频尺寸；改为主视频 geometry contract。

## 验收条件
- [ ] 关闭属性浮窗时原视频、拾取模式、草稿保持；旧脚本库只读检查不会异常。
- [ ] 源代码不再有供自动化操作使用的 Inspector 第二播放器。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
禁止未经对照验收就一并删除基于视频的 drag/coordinate 业务实现。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
