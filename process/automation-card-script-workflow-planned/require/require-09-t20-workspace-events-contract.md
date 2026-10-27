# T20 · FINAL V3 手机卡片浮窗、共享草稿与动作事件契约

> **2026-10-27**｜交付分支：`feat/automation-center`｜状态：**公共 TypeScript 合同及原选中入口校验接线已提交，未经测试**。  
> 依赖 T19：`require-08-t19-baseline-audit.md`；产品最终要求：`require-05-final-card-script-workflow.md`；源码：`easytier-web/frontend/src/modules/automation-floating-contract.ts`。

## 一、唯一数据与 UI 所有权

| 领域 | 唯一真实持有者 | 生命周期与清理时机 | 禁止做法 |
|---|---|---|---|
| 原实时画面、视频像素映射 | `PhoneCard.vue` 的 `usePhoneSession`、`primary-video` | per owner/device，随实际视频连接/断开；全屏切换**不重建** | 浮窗里另建 `<video>` / WebRTC `usePhoneSession` |
| 手机授权/当前绑定 | `AutomationCenter.vue` 获取的 `devices[]` + 后端 | owner/device/fingerprint 变化即让旧异步任务/拾取无效 | 前端只按手机名称判定设备身份 |
| 当前工作脚本步骤 | `AutomationCenter.vue::draftWorkspace` 唯一 reactive 对象 | per owner；用户确认新建/切换/克隆才原子加载/替换 | 每张 PhoneCard、浮窗、ScriptLibrary 各有第二份可写步骤 |
| 服务端草稿 CAS | 同一份 `draftSync.state`，id+revision | 保存成功更新；冲突前端不可假报已同步 | 误把正式 Script revision 当作 Draft revision |
| 正式脚本记录 | Script API（scriptId + scriptRevision） | 保存/发布和克隆确认后由服务端分配身份 | 用名称或工作草稿 ID 当脚本 ID |
| 浮窗主/子层 | `AutomationFloatingUiState`，**per device 只展示 UI** | 图标开关、浮窗拖动、进入/退出全屏仅调整布局；脚本运行仍持续 | UI state 内保存步骤、参数、MediaStream 或动作锁 |
| 单步运行状态 | Runner API + `requestId/runId/deviceId/stepId` 回执；前端只展示 | 提交时冻结快照；失败/未知需显式查询或人工新发起 | 把主浮窗开关当作任务启动/停止、断线自动重新执行 |
| 完整脚本执行目标 | **工作台已勾选且有授权的 `selectedIds`** | 用户点击全局 ▶ 时冻结设备集合 | 把打开主浮窗的 `deviceId` 误认为全部执行目标 |
| 只读/写操作租约 | Rust / Executor，T36～T39 后续实现 | 真实动作排他；UI 不能授予写权限 | 删 `Journal.claim` 或靠禁用按钮当安全保障 |

## 二、类型接口：已作为公共模块落地

`automation-floating-contract.ts` 交付：

- `AutomationCardScope`：`ownerId/deviceId/targetFingerprint/sessionId`。
- `AutomationScriptScope`：`scriptId/scriptRevision` + `draftId/draftRevision` **两套身份**。
- `AutomationFloatingScope`：在以上身份外追加上层单调递增的 `generation`；用于异步请求的陈旧保护。
- `AutomationFloatingUiState`：某个设备的浮窗开关、前置层和坐标，只允许 UI 属性；不保存脚本/参数。
- `AutomationFloatingCommand`：`panel.open/close`、`step.edit/delete/trial/repick`、`script.open/clone/create/run` 的**带 scope 的严格事件联合类型**。
- `canAccessAutomationCard()`：同步校验 owner、device id 与工作区 owner；本轮已接入原 `AutomationCenter.selectCardStep()`，以统一步骤选中归属保护。
- `captureAutomationFloatingScope()`：快照读取当前设备、脚本及草稿 CAS 身份，允许媒体 session 为空（离线编辑）。
- `automationFloatingScopeIsCurrent()`：异步结果回来时，严格比较 owner、设备、fingerprint、Script/Draft ID 和两个修订及 generation；涉及主视频坐标/控件命中时使用 `requireLiveMedia: true` 再校验 `sessionId`。

**注意：这些类型与辅助函数定义了前端 UI 合同，不拥有手机动作写权限。** 每次执行仍需后端验证完整权限与当前绑定，且操作设备与正式脚本提交的幂等由后续专用任务落实。

## 三、事件流与源数据

| 事件 | 用户触发 | 发送/接收 | 必须携带 | 校验/结果 |
|---|---|---|---|---|
| `panel.open/close` | 每张手机的 orb“自动化”按钮 | PhoneCard → Center / 可视化 host | 当前 card scope | 不发 ADB，不关闭视频；menuOpen 与浮窗 open 独立 |
| `script.create` | 浮窗下拉首行输入+新建 | selector → Center → Script API | scope、name | 0 步真实记录，成功回执后修改共享草稿；取消无写入 |
| `script.open` | 选择旧脚本 | selector → Center → Script API | scope、scriptId | dirty/CAS 检查，通过才原子更新名称/步骤/修订 |
| `script.clone` | 点击旧脚本的克隆并修改名确认 | clone 子窗 → Center → Script API | scope、原 scriptId、新名称 | 新 ID，新 revision；不复制历史、参数秘密；回执后选中 |
| `step.edit` | 点正文或 ✎ | timeline → Center | scope、stepId | 检查归属、stepId、未保存保护；同一份工作草稿；不能重复录入 |
| `step.delete` | 每步 🗑 | timeline → Center | scope、stepId | 普通删当前，条件先校验影响范围；同步 CAS；不可更改 run 快照 |
| `step.repick` | 浮动编辑窗重新拾取 | editor → Center/Picker | scope、stepId | media scope+generation+旧树快照校验，只替换同一步 |
| `step.trial` | 卡片浮窗每步 ▶ | timeline → Center → Runner | scope、stepId、请求身份 | **只执行源 deviceId**，后台准备；失败/未知不自动重试 |
| `script.run` | 浮窗全流程 ▶ | panel → Center → Runner | scope、requestId、**已选设备 ID 列表** | 与源 cardId 分离，冻结脚本与列表；逐机授权/写锁 |
| `fullscreenchange` | 进入/退出全屏 | Browser → PhoneCard/floating host | 当前 screen element；原 scope | 保持原 video/sessionId，迁移主子窗宿主与位置，不重建草稿 |

## 四、判定失效（两类，不能混淆）

**离线也能管理脚本：** 只需有效 owner/device 与 Script/Draft ID 修订；媒体 `sessionId` 为空时允许显示旧步骤和编辑参数。但 **定位高亮/拾取/真实动作必须有当前视频及绑定**，并经过现有后端授权和硬校验。

**需要像素与真实设备的操作：** 保留 `targetFingerprint` + `sessionId` + geometry/snapshot generation 的全部匹配；脚本正式 ID 或草稿 ID/revision 变了、手机重新绑定、账号切换、媒体会话变了、异步任务 generation 过时，必须阻止旧回调落地。

| 事件 | 清理/保持策略 |
|---|---|
| 手机全屏进出 | **保持**唯一原 MediaStream、原步草稿、运行回执、编辑内容；仅重新限制浮窗坐标 |
| 手机主视频断开 | 可保留 UI/已确认草稿；禁真实动作/只读定位，显示不可用；不自动无线 ADB 配对 |
| 自动化浮窗关闭 | 只关闭 UI 可见性，不关闭视频、不卡住旧运行、不断开草稿同步 |
| 脚本切换/克隆 | 先保护 dirty/CAS；成功后更新 Script/Draft 身份、失效旧编辑/拾取回调 |
| 手机重绑/owner 切换 | 旧 scope 异步回调必拒绝；不可复用旧 video session/执行权限，账号数据隔离 |
| 单步或整套运行开始 | 冻结请求 UUID、设备集合、步骤快照；panel 可关闭，run 继续由服务器负责 |
| 当前动作未知/清理待确认 | 仍可按授权只读观看，**不能**重试或恢复手控，直至后端确认 |

## 五、分阶段交接与测试责任

1. **T21～T24**：只消费公共 UI state/scope；实现 orb、主子窗、全屏 host；禁止新建视频和 Script store。
2. **T25～T28**：消费 Script/Draft 两套身份/异步 guard，0 步创建与 CAS；成功后才替换共享工作区。
3. **T29～T35**：步骤编辑/删除/Inspector 统一 `stepId+scope`；涉及视频必须传 `requireLiveMedia`。
4. **T36～T45**：Rust/Executor 独占 lease 与 idempotent run ID，前端 event contract 不能绕过后端互斥。
5. **T46～T51**：替代后删除旧 UI，测试时覆盖跨手机、全屏、长延迟请求和账号切换。不能把合同提交当实测通过。

### 仍待开发的事项

- 本任务**没有**直接实现浮窗 UI、运行/观看权限、离线脚本发布、克隆 API、0 步保存或全屏 portal；各项按后续 T21～T51 执行。
- 本次对共享选中调用只做等价 owner/device 工作区校验，**未运行 TypeScript/单测/构建/浏览器/真机验证（调试模式）**。
