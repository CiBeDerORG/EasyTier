# P12 · 视频观看与动作写锁分离

> **状态：跨层代码已提交、待联调及实机验证**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
确保自动准备、UIA 初始化和脚本运行期间，原视频持续显示；手控/准备/脚本动作权仍遵守每机独占与完整清理。

## 前置条件与关联任务
- 前置：P07；可与 P08/P09 并行。
- 开发任务：[T36](../task/task-36-lease-contract.md)、[T37](../task/task-37-executor-lease.md)、[T38](../task/task-38-rust-lease.md)、[T39](../task/task-39-frontend-readonly-lease.md)。

## 交付要求
1. 冻结 viewer_lease 与 action_lease 状态图：owner、device、fingerprint、session、holder、lease generation、clear/unknown。
2. Executor 在 media/session 与 manual/prepare/script claim 上解耦写操作和只读观看，禁止第二个控制者写命令。
3. Rust active_session/run_occupied 判定只让真实 action owner 拦阻 run；viewer 仍需授权，并保证创建、移交、取消、清理失败时 fail-closed。
4. 前端复用现有 phone video 与 sessionId；执行开始禁 tap/text/key/unlock（不仅 UI）、完成清理确认后恢复。
5. 禁止因简单跳过 releaseOwnedTrialPreviews 或删 Journal.claim 造成同机两个可写者；连接异常视频中断应显式呈现而非假称持续。

## 可验收判据
- [ ] 运行中原画面真实显示变化，手控 API 不能穿透，执行结束或未知状态权限处理正确。
- [ ] 两浏览器争用、断线、执行器重启、设备重新绑定都不会产生重复动作。

## 边界与风险
涉及 Executor/Rust 两个权威服务，跨层提交前接口和状态机先评审；严格不以低质量截图/第二流替代持续观察目标。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。

## 2026-10-28 · 实际开发进度
- T36：写入 [状态机与错误合同](../require/require-10-t36-viewer-action-lease-contract.md)，保留原视频、owner/指纹、两阶段降权与脚本动作独占。
- T37：Executor `Journal.claim` 加固，STOP/TOUCH RELEASE 受独占仲裁，`view-only` / `resume-manual` 保留原 WebRTC 并 fail-closed；错误降权重试必须确认完成手控资源释放。
- T38：Rust `automation_sessions` 新增 `suspending/view_only/resuming` 事务状态；`create_run` 仅豁免真实已确认的本账户/设备/指纹 viewer；WebSocket/解锁双层硬拒写。
- T39：原卡片及 `CardTrialPanel` 单步试跑接入不关视频的只读移交、同会话校验、手控禁用及显式恢复；批量入口和旧脚本库暂保留旧安全交接方式，留给 T40/T43/T45 独立迁移。
- **所有原验收复选框保持未勾选**：按调试模式未运行 TypeScript、Rust/Python 检查、前端测试、构建、CI 手工触发、浏览器及 FnOS/Android 真机验收。不同组件的提交不等于端到端闭环已验收。
