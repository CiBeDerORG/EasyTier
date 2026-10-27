# T49 · Rust + Executor 观看写锁/空脚本/幂等自动测试

> 状态：**待开发**｜计划：P16｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**QA 后端 Agent**。
- 文件边界：Rust automation/db/restful 测试文件、automation-executor tests；不直接碰生产互斥文件。
- 开始条件：T25、T37、T38 完成后。

## 实现步骤
1. Rust 0 步脚本可保存但 create_run 被拒；revision/owner 防越权兼容。
2. Executor 同一 device 的 viewer 与 action write holder 并存、跨页重复写拦截、cleanup_pending 拒绝新动作。
3. 网络未知和 stale fingerprint 不重放、不窃取其他页 session；设备重启/手控释放竞态。

## 通过标准
- [ ] 形成设备 lease 与 CAS 场景测试矩阵，提供失败日志的脱敏路径。
- [ ] 验证动作排他可靠后才允许试跑不断流正式上线。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
严格不借测试修改 Journal.claim 权限模型；未授权时不实际启动测试/构建。

## 本任务当前状态
只建立任务计划；未修改功能代码，未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。
