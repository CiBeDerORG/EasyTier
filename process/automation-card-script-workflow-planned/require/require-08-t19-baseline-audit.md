# T19 · FINAL V3 当前代码基线审计与替换安全门槛

> **2026-10-27**｜`feat/automation-center`｜审计基线：`91392fd26e001071a1d4d77b4edb5f96e9bdfbb5`｜**审计已完成；不代表需求已开发/测试**。  
> 产品最高合同：`../require/require-05-final-card-script-workflow.md`。产品取舍：`../require/require-07-product-frontend-simplification.md`。

## 一、真实代码入口及业务所有权

| 旧入口与责任文件 | 当前真实调用/状态 | 最终替代入口 | 删除门槛 | 验收 |
|---|---|---|---|---|
| `PhoneCard.vue` 主 video / `usePhoneSession.ts` | `<video ref=video>` 绑定原 `session.stream`，手控、Picker、geometry；浏览器 fullscreen 为 `phone-screen` | **继续原样作为唯一操作画面**，浮窗作为视频上方 UI | **永久保留主视频与会话控制**；不得把背景 blur 的 ambientVideo 误识别为另一个操作会话 | FF-01/06/07/14/15 |
| `PhoneCard.vue` `phone-controls` | orb 展开三组按钮；`side-actions` 为停止/全屏/唤醒/录制；有 6 秒菜单收起 | 同一 orb 的自动化 icon 占原唤醒槽，唤醒上移；控制独立的浮窗显示 | 新按钮打开/关闭完全可用、且不误触发送 ADB，原唤醒可用 | FF-02/03/04/06 |
| `PhoneCard.vue` footer 下 `CardStepTimeline.vue` | 共享 `draftWorkspace` 的步骤投影，行点击/✎ 现在都只发 `edit`，▶ 打开卡片试跑 | 同一个 `CardStepTimeline` 移入脚本主浮窗，增加每行 🗑 | 新窗口脚本名称、步数、分支、编辑/播放/删除全部可用后，才移除 footer 固定渲染 | FF-03/08/09/11 |
| `PhoneCard.vue` 内嵌 `CardStepEditor.vue` | 从上层 `cardStepEditor` 选中步骤后显示；调用 `ScriptStepEditor`；回传 dirty/save/repick/demo | 主视频上的可拖动步骤编辑子窗，共用原参数校验及回传事件 | 未保存参数保护、重拾同 ID、只读演示和 CAS 结果都恢复后才删旧内嵌外壳 | FF-05/07/11/17 |
| `UiInspectorDialog.vue` | 现有 `Dialog modal append-to=body`，持有 `<video ref=video>` 接 `props.stream`；具备候选、坐标、拖拽、属性/录入 | 只显示控件树/属性/动作浮窗，拾取与高亮来自主视频有效像素 | **不能仅删除 video 标签**；必须先用主视频替代命中/高亮/拖拽起终点/旋转与快照校验 | FF-01/05/06/15 |
| `ScriptLibrary.vue` `scripts` tab | 完整正式脚本侧栏、ScriptStepEditor、步骤管理、正式发布、单步/全局运行及安全预检 | 复用 Script API/CAS/表单/运行状态；日常脚本管理迁原视频主浮窗 | 克隆/发布/分支/多设备运行/失败恢复等全部从新入口可用后才移除日常编辑外壳；旧深链兼容需评审 | FF-08～14 |
| `ScriptLibrary.vue` `previewPlayer` | `previewStream` 经 `watch` 绑定弹窗独立的 video，预检前后可能关闭已有卡片 session | 每台设备使用自己原 `PhoneCard` MediaStream，弹窗不再有第二个操作 video | T37/T38/T39 完成观看/写锁分权，并且同视频试跑/整套执行验收后再去除 | FF-01/13/14 |
| `CardTrialPanel.vue` | 含自动准备/独立风险确认/重复“确认执行”、回执恢复/未知结果/失败修复；`PhoneCard.openCardTrial` 全屏下主动 `exitFullscreen` | 浮窗行 ▶ 一次点击开始，自动准备/按需填写参数，结果/修复内联 | 先提取运行回执、幂等、安全确认，再替换 modal；未知结果禁止再次自动发送，取消不谎报完成 | FF-06/12/14/17 |
| `PhoneAutomationPanel.vue` | 主要为 UiAutomator2/组件准备查询、历史、初始化、取消与日志 | 保留准备与查询领域能力，后续从自动化浮窗显示当前状态 | **不可被当成重复脚本编辑器直接删除**；可替换弹窗外壳但必须有准备/状态查询替代入口 | FF-12/14 |
| `CardRepairEntry.vue` / `RunHistory.vue` | 失败回卡片/脚本库编辑，多处 `changeTab('scripts')` | 运行历史继续可见；失败点击定位到卡片视频上相应步骤浮窗 | 需先保留快照修订和原 device/step 归属，避免失败修复指向另一个手机 | FF-12/13/17 |
| `AutomationCenter.vue` | **当前唯一 owner 级 `draftWorkspace`**；单次 `draftSync`；`cards` per-device 会话；`inspectorDeviceId`；`cardStepEditor`；`changeTab` 释放部分会话上下文 | 全局只保留一份 script/draft，卡片自己的浮窗打开状态与运行权限；编辑浮窗唯一活跃 | 禁止重新 `new` 第二个 Draft 或 Session；`scripts` 跳页的编辑路径须在新浮窗替代后撤销 | FF-01/09/16/17 |
| `automation-run-handoff.ts` | `releaseOwnedTrialPreviews` 保守地只释放本页拥有的预览，并在执行前重新确认服务器占用 | 新 viewer/action 协议提供真正的只读媒体不断流和脚本写锁移交 | **现阶段不得移除该模块或绕过其保护**；Rust + Executor 状态机和双浏览器回归完成后才能迁移 | FF-14/17 |

## 二、已查清的现状事实及隐含缺口

1. `CardStepTimeline` 的双事件（选中 + 编辑）已由此前 R0 去重：一次 `edit` 由 `AutomationCenter.editCardStep` 内统一调用 `selectCardStep`。本次不重新更改该流程。
2. `UiInspectorDialog` 的第二个 video 目前复用主 MediaStream，并不新增 WebRTC 会话，但**是第二个操作画面 UI**；仍与 FINAL V3 禁止重复播放器的目标冲突。
3. `ScriptLibrary.vue` 的 `previewPlayer` 亦会显示原卡片 stream，`prepareRunDevice` 与 `CardTrialPanel.prepare` 还会释放本页预览。不能不修改独占协议就简单删关闭代码。
4. `db/automation/validation.rs` 的 `steps` 校验是 **1～100 步**，而 FINAL V3 要求新建后立即保存 0 步脚本；专由 **T25** 兼容 Script API，**运行仍应拒绝 0 步**。
5. `PhoneAutomationPanel` 和 `InspectorPropertiesPanel` 均有有用能力，清理只涉及重复布局容器，**不应删除其核心功能**。
6. 同一用户账号内只有一份 `AutomationDraftWorkspace` 和 `draftSync`；但不同手机的视频/全屏/手控/试跑状态按 deviceId 独立存在，子浮窗必须与目标设备显式绑定。

## 三、替代前严禁删除的模块

`usePhoneSession` / 主 video、`automation-draft-sync` / Script API / CAS、`ScriptStepEditor` / `InspectorPropertiesPanel`、`automation-locator-candidates` / `automation-pick-engine` / `automation-locator-demo`、`automation-trial-receipt` / RunHistory / 历史快照、`automation-device-compatibility`、`Journal.claim()` / Rust 授权与 cleanup_pending/outcome_unknown。

所有新旧切换必须遵循 **替代能力已接线 → 条件分支/全屏/跨设备/后端权限可验 → 去除冗余呈现 → 再清理无引用代码**，不能用删除功能的方式消除错误。

## 四、T20 交接与风险

- 需要统一 owner/device/targetFingerprint/sessionId/stepId、scriptId/revision、draftId/revision 以及请求 generation 的只读身份合同。
- 必须区分 **共享脚本数据域**、**每卡片浮窗 UI 状态**、**媒体 session 状态**和**服务器真实动作 run** 四类生命周期，切账号/设备解绑/换脚本/换投屏/全屏时废弃的状态不相同。
- 编辑参数允许在 ADB 暂时离线时进行（owner/脚本身份仍需有效）；真实定位高亮与写入动作必须有当前有效 video session/绑定。
- 所有执行命令的安全检验仍由前端、Rust、Executor 多层执行，UI 事件合同不表示获得设备写权限。

## 五、状态

**T19 审计文档已完成**（在代码基线 `91392fd` 上分析）。审计没有运行本地/远程测试、类型检查、构建或真机操作；所列 FF/AC 是未来验收门槛，**不是已通过验收**。
