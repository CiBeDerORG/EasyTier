# T38 · Rust 会话占用与脚本调度观看写锁分层

> 状态：**待开发**｜计划：P12｜复杂度：L｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
