# T47 · 失效事件、样式、兼容路由与前端模块瘦身

> 状态：**待开发**｜计划：P15｜复杂度：S-M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**前端架构/集成负责人**。
- 文件边界：AutomationCenter.vue、PhoneCard.vue、ScriptLibrary.vue、frontend/modules 旧独占模块；各组件独占人逐个交接。
- 开始条件：T46。

## 实现步骤
1. 检索并删除替代后失效的 event/prop/import/CSS 及无引用 DOM 选择器。
2. 保留旧访问链接必要兼容：原 scripts Tab 可导航回主视频浮窗，而不是留下另一个未维护运行逻辑。
3. 优化集中状态、减少巨型组件内部的重复条件/手控与浮窗开关混杂，分解 composables，避免新抽象比旧代码更多。

## 通过标准
- [ ] 所有旧 UI 剩余调用有归属和错误处理，不存在悬空状态。
- [ ] 不改变录制、群控、手控、ADB 查询等非 FINAL 业务的表现。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
只有有证据证明功能替代后才删除，保留 owner 检查、幂等、异常清理和安全代码。

## 本任务当前状态
只建立任务计划；未修改功能代码，未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。
