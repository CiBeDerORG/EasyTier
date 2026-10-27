# T36 · Viewer 与 Action Lease 状态机及接口合同

> 状态：**状态：设计合同已提交；运行验收待完成**｜计划：P12｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Rust/Executor 架构负责人**。
- 文件边界：新增 plan 技术补充与 Rust/Executor 共享接口文档；不直接跨文件改两套服务。
- 开始条件：T20。

## 实施清单
1. 规范 device/fingerprint/owner/session、viewer（只读）和 manual/prepare/script（独占写）状态及原子交接。
2. 明确 viewer 保持原 WebRTC、action 写权限申请/释放、cleanup_pending 与 outcome_unknown 的不可写保护。
3. 定义断连/并发浏览器/切绑定/脚本取消/设备重启/连接过期的错误类型、幂等和恢复策略。

## 验收条件
- [ ] 提供 Rust、Executor、前端共享的接口和状态转移表，列互斥安全不变量。
- [ ] 每机任一时刻最多一套动作写 holder，读屏永不自动获得触控权限。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
技术前置合同，不能简单删 Journal.claim 或 active_session/run_occupied 硬阻断。

## 验证状态
计划阶段描述已由下方实际交付记录补充；代码已提交、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T36 交付说明
- 已提交 [viewer/action lease 跨层协议合同](../require/require-10-t36-viewer-action-lease-contract.md)（提交 `553e188`），定义 `active → suspending → view_only → resuming → active`、单设备/ADB endpoint 唯一写 holder、同 sessionId 原视频、两阶段防竞态迁移及未知态 fail-closed。
- 已在后续 T37～T39 按合同接线实际代码，但合同本身不是执行安全或真机验收证据。
- **未执行**测试、TypeScript、Rust/Python 检查、构建、CI、浏览器或 FnOS/Android 真机验收（调试模式）。
