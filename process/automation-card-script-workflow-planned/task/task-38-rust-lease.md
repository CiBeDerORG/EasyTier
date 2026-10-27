# T38 · Rust 会话占用与脚本调度观看写锁分层

> 状态：**状态：业务代码已提交，待端到端联调验收**｜计划：P12｜复杂度：L｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Rust 服务 Agent**。
- 文件边界：easytier-web/src/db/automation/runs.rs；restful/automation 会话及 run 接口（独占 Rust 写入）。
- 开始条件：T36、T37 接口兼容。

## 实施清单
1. 区分 owner-scoped viewer session 与独占写动作 holder，调整 active_session/run_occupied 互斥规则。
2. 运行创建及手动动作授权按 deviceId/fingerprint/session/revision 进行服务端原子校验，避免只凭前端灰按钮。
3. 清理 pending、脚本结束/取消、请求网络未知、一机两浏览器争用保持幂等与回执一致。

## 验收条件
- [ ] 合法只读浏览不阻止脚本执行，非法跨 owner、双写者永远无法通过。
- [ ] Rust API 返回准确授权/占用/清理未知状态，便于前端行内反馈。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
必须与 Executor T37 进行状态机和 API 一致性审查，不能单独上线半套。

## 验证状态
计划阶段描述已由下方实际交付记录补充；代码已提交、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T38 实际代码交付
- `db/automation/mod.rs`：事务内 `begin_session_permission` / `confirm_session_permission` 实现 suspending/view_only/resuming；同一账户与目标指纹 CAS，恢复手控必须先确认无 pending run；viewer 状态可续期且仅作为媒体观看授权。
- `db/automation/runs.rs`：脚本创建 BEGIN IMMEDIATE 仅忽略 `state=view_only` 且同 owner/device/fingerprint 的既有观看会话，其他任何活动、过渡、跨账号及 cleanup_pending 会话仍硬阻断。
- `restful/automation/sessions.rs`、`mod.rs`：注册单一原画面的只读移交与手控恢复 API；校验原 actor、执行器 identity、设备目标与双向回执；所有 unlock 与 WebSocket 写命令按权威当前 state 拒写。执行器返回未知保留过渡拒写，不假认已移交。
- 已查阅 `m20261024_000018_automation_center.rs`，`automation_sessions.state` 为 TEXT（无枚举 CHECK）；已有 partial UNIQUE 观看会话索引依旧约束每机一条媒体会话，无需数据库迁移。
- **待验证**：实际 Rust 编译、SQLite 两页面并发 race、执行器回执超时/重复请求、重启旧会话、授权撤销及目标变更。
- **未执行**Rust 编译、测试、构建、CI、浏览器或真机操作（调试模式）。
