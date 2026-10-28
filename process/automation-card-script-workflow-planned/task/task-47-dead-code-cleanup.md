# T47 · 失效事件、样式、兼容路由与前端模块瘦身

> 状态：**第一批旧路由/样式瘦身代码已提交，待检查及联调**｜计划：P15｜复杂度：S-M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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
第一批兼容路由入口与死样式已提交；未执行检查、测试、手动 CI 或真机操作。后续执行遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T47 第一批

- `ScriptLibrary.vue` 保留旧链接和正式脚本管理历史兼容，但新增“回手机原画面脚本浮窗”入口，仅把已授权选中设备 ID 交给 `AutomationCenter`；不启动新视频或旧试跑；无设备时明确说明。已删除该页无模板调用的 `.recording-notice` 样式。
- `AutomationCenter.vue` 新增 `openLegacyScriptOnCard`：进入 workbench 后通过现有 `PhoneCard.openAutomationPanel` 实例打开同一浮窗，保留设备 owner/ADB 授权核验与异步账号变化防护；必要时清除筛选防止打开不可见卡片。录制、群控、ADB 连接与真实脚本执行接口没有修改。
- 保留 `automation-run-handoff.ts` 等旧共享模块直到能够证明不存在其它前端/历史回归引用，避免误删仍在用的安全代码。T47 尚需对更多失效 props/events 和旧链接逐项核实；旧脚本兼容管理 Tab 不能在未验收前删除。
- **未执行** 编译、TypeScript 检查、lint、单元测试、CI 手动触发或 FnOS/Android 真机测试；静态查看组件结构和引用不等于功能/类型检查通过。
