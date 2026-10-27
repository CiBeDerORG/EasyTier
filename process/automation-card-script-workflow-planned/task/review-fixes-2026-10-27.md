# 2026-10-27 · T01–T18 全局代码审核问题修复记录

**基线：** `feat/automation-center` 上的审核结果（审核基线 `765fe64`），不做重构，不修改产品的 12 项 AC 定义。
**状态：** 七项审核缺陷 **代码修复已提交，待新 CI 和实机验证**。该表不等于缺陷经真机关闭，也不代表 T18 已验收。

| ID | 等级 | 现象与技术根因 | 修复模块 | 验证状态 |
|---|---|---|---|---|
| P0-01 | P0 | 卡片 `POST /runs` 已送出时关闭弹窗/切路由，迟到回执丢失原 UUID，可能再次创建动作 | `automation-trial-receipt.ts`、`CardTrialPanel.vue`、`runs.rs` 只读 `request_id`；同类风险加固 `ScriptLibrary.vue` | 代码已提交、自动测试/真机待核对 |
| P1-01 | P1 | 执行器正常返回 `action_sent` / `assertion_passed` / `condition_passed`，卡片汇总误判为 failed | `automation-run-result.ts`、`useCardRunStatus.ts` | 代码已提交、自动测试/真机待核对 |
| P1-02 | P1 | 准备未知恢复成 ready，却未重新获得跨机兼容性审查 | `CardTrialPanel.vue` 只读准备恢复后重采当前设备/层级、重新人工确认 | 代码已提交、真机待核对 |
| P1-03 | P1 | 异步 `GET /draft` 返回时覆盖等待期间新增/修改的步骤 | `automation-draft-sync.ts` 绑定开始时快照、ID/revision/pending/phase 并在应用时重验 | 代码已提交、双浏览器待核对 |
| P1-04 | P1 | 前端屏幕尺寸严格对比，但执行器允许等比例坐标缩放 | `automation-executor/automation_executor/steps.py` 真实发动作前要求宽高精确一致 | 代码已提交、Python/真机待核对 |
| P2-01 | P2 | 拾取关闭时浏览器右键依然被 preventDefault | `PhoneCard.vue` 只在 `pickingPhase==='on'` 且可拾取时拦截 | 代码已提交、浏览器待核对 |
| P2-02 | P2 | 工作草稿已同步而卡片却显示“草稿尚未保存” | `card-step-projection.ts` 分离“正式脚本是否发布”和“服务端工作草稿同步” | 代码已提交、UI待核对 |

## P0 安全边界

- **先存请求身份再 POST：** 使用浏览器 `sessionStorage` 按 `ownerId+deviceId` 存储 UUID、步骤 ID 与时间；**绝不存储**脚本文本、selector、请求 body、账号 token、输入参数或密码。
- **不自动重放：** 旧 UUID 存在时新开卡片、恢复页面或脚本库新运行均阻止新请求；只用 `listRuns()` 的 GET 且同时匹配请求 ID 与设备 ID 追溯服务器已创建的 run。找不到并不证明没执行，仍保持保护。
- **服务端权威排他保留：** `create_run` 在 `BEGIN IMMEDIATE` 内检查 preview 与脚本占用；执行器 Journal 的 `cleanup_pending` 仍不得被 UI 绕过。
- **多设备同一请求身份：** 脚本库单机/批量提交前为所有目标设备存相同 UUID；在所有标记安全写入之后才 POST。
- **有限的浏览器保障：** `sessionStorage` 只在当前标签页会话内持久，不能取代跨设备的服务器幂等表和控制权互斥，也不能恢复未知动作时的用户输入参数。浏览器数据被人为清理、私密模式禁用存储时新请求 fail closed。超过服务器默认 100 条近期历史的旧请求可能无法在当前 UI 解除保护，需另行人工核对服务器运行日志。
- **警告：** 通过旧 run 的回执只说明该请求已登记，**不**等于设备清理完成或动作未发生。UI 不提供忽略此警告而自动生成新动作的入口。

## 新增回归测试

- `frontend/tests/automation-trial-receipt-regression.test.mjs`：请求身份跨组件重开保留、跨账号隔离、拒绝另一个 UUID、失效 storage fail closed、只读匹配 run/device、无敏感 payload 落盘。
- `frontend/tests/automation-run-result-regression.test.mjs`：`action_sent` 与业务校验区分、assert/condition 通过、失败/未知结果优先级、正在运行的脚本状态。
- 这两组用例已接入现有 `pnpm test:unit`。修改后新的 GitHub 自动构建结果仍需与提交 SHA 对应核查；旧版 125/125 不是本轮新增用例通过的证据。

## T18 仍必须真实验证的场景

1. 在真实 Android 的触控执行请求 `POST /runs` 飞行过程中关闭弹窗、刷新、切页：从只读历史按 UUID 找回，不能生成第二个动作。
2. 使用两浏览器争用相同 ADB 设备，脚本库多机提交其中一机占用时所有目标均不得发生绕过。
3. 脚本运行结果 `action_sent` 与 `assertion_passed` 能正确显示；未知结果不可再次操作。
4. 在恢复服务端草稿的延迟 GET 期间录入第 5 步，旧响应不能覆盖新步骤；不同账号和两个浏览器的 CAS 仍有效。
5. 1080×2400 脚本移植到 720×1600 时 UI 阻止、即便直接 API 调用，执行器仍不缩放点击坐标。
6. 拾取关闭时浏览器右键行为、拾取开启时右键只读，以及卡片状态“草稿已同步但未发布”文案。
7. **不在当前调试模式自动执行**本地测试、手动 CI、部署和真机动作。

## 交付状态

代码分支：`feat/automation-center`。所有补丁按中文提交信息增量推送。**真正关闭缺陷**需要后续 GitHub 自动 CI 最新提交通过，以及 T18 用户授权的 FnOS+Android+浏览器端到端证据。
