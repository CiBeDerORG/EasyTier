# T47 · 失效事件、样式、兼容路由与前端模块瘦身

> 状态：**第二批无用导入/参数与导航状态已收敛，待联调验收**｜计划：P15｜复杂度：S-M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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

## 2026-10-28 · T47 第二批静态取舍和代码交付

- **删除真实无用引用**：`useFloatingBatchRun.ts` 去除旧准备 helper 未使用的 import `requiresAutomationHelper`；`ScriptLibrary.vue` 删除未使用的 `deviceNames` prop，并在父层删除传参，保留仍用于群控/设备反馈的共享 `AutomationCenter.deviceNames`。
- **合并重复目标选择**：脚本兼容页移除第二份 `targetCard` 推导，直接复用原 `defaultPickerDevice`，在原选中授权设备集合内确定跳回目标。
- **修复导航与控件编辑边界**：旧脚本 Tab 返回工作台时，原实现先 `cards.get(deviceId)` 再清过滤器，隐藏设备必然没有挂载的卡片引用。现调整为先恢复可见，再 await nextTick 后重新确认 owner/连接状态和原卡片引用；旧 ScriptLibrary 的控件重新拾取同样先恢复原卡片可见并核对目标 fingerprint，未确认不执行拾取。
- **消除多路径重复**：`AutomationCenter.vue` 新 `revealOriginalAutomationCard` 仅修改本页筛选（不建立媒体或控制会话），被旧控件拾取、旧脚本页导航、历史失败卡片回修三个入口复用；每个调用点仍自行检查授权/步骤快照/实际 session。
- **恢复持久保护**：`automation-script-workspace.ts` 的 owner watcher 改为 `immediate`，避免工作区一开始就带有效 owner 时没有及时展示旧正式 POST/PUT/删除未知结果凭据。HTTP 状态未知继续只读核对、禁止自动重发。
- **不能删除的旧模块**：`automation-run-handoff.ts` 的 `releaseOwnedTrialPreviews` 虽从新主业务路径移除，但 `frontend/tests/automation-run-safety-t17.test.mjs` 仍直接导入并覆盖设备级安全移交回归；没有重写等价测试前不删除。 `PhoneCard` 的 `dock-left/dock-right` CSS 看似无对应字面 class，实际由 `:class=` 动态生成，故保留。脚本兼容 Tab 仍需 FnOS 和真实 Android 验收后才能退役。
- **静态阅读范围**：直接阅读关联组件、真实 frontend/tests 安全引用、条件树/共享 helper，未执行自动化扫描、类型检查、lint、编译、CI 或真机。动态样式、外部测试、历史入口不可仅凭字符串检索随意删除。
- **结果**：小提交已推送 `feat/automation-center`；T47 主体进一步瘦身但仍**待联调**，不勾选验证通过项。
