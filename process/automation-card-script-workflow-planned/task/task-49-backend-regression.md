# T49 · Rust + Executor 观看写锁/空脚本/幂等自动测试

> 状态：**执行器原视频只读租约与写锁测试代码已提交，尚未执行**｜计划：P16｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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

## 2026-10-28 · T49 第一批 · 真实 Runtime + SQLite Journal / 模拟媒体边界

- 新增 `automation-executor/tests/test_view_only_lease_t49.py`，复用真实 Executor `Runtime`、持久化 `Journal`、实际 `claim/release` 和会话方法；仅模拟媒体通道与外部设备依赖，测试不会发送 ADB/UiAutomator2 命令，也不会关闭真实媒体。
- 测试覆盖：（1）`view_only` 释放它自己持有的 manual 写锁前向原媒体触发 touch.release，但**不关闭媒体**；（2）脚本 action holder 可以与已确认 viewer 并存，viewer 的 `resume_manual` 在脚本占用期间拒绝，释放后仅恢复资格、不自动 claim 或发送手机动作；（3）worker 清理未知时保持 `cleanup_pending`，第二次请求不得把失败变成功；（4）仅关闭观看会话不释放属于脚本 run 的写锁；（5）外部 holder 占用时首次 view_only 不能夺取写权限；（6）错误 executor ID 不能降级或恢复手控。
- 该批测试**没有修改** `Journal.claim`、`Runtime.view_only` 或实际锁模型，避免以测试为由放宽互斥条件。现有 T17 测试文件继续保留。
- 尚待 Rust 主服务对零步正式脚本可保存/运行拒绝、CAS 409/账号越权、`viewer_only` 转换前后数据库状态的回归，且待执行器模拟用例和 FnOS/手机真实测试。**未执行** Python unittest、Rust 编译/测试、CI 手动触发、设备/浏览器验证。
