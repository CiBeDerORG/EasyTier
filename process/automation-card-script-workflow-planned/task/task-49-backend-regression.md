# T49 · Rust + Executor 观看写锁/空脚本/幂等自动测试

> 状态：**Rust DB + Executor 租约回归源码已提交，已在自动CI通过，真机未验证**｜计划：P16｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

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
测试代码已分批提交，未执行检查、测试、手动 CI 或真机操作；继续遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T49 第一批 · 真实 Runtime + SQLite Journal / 模拟媒体边界

- 新增 `automation-executor/tests/test_view_only_lease_t49.py`，复用真实 Executor `Runtime`、持久化 `Journal`、实际 `claim/release` 和会话方法；仅模拟媒体通道与外部设备依赖，测试不会发送 ADB/UiAutomator2 命令，也不会关闭真实媒体。
- 测试覆盖：（1）`view_only` 释放它自己持有的 manual 写锁前向原媒体触发 touch.release，但**不关闭媒体**；（2）脚本 action holder 可以与已确认 viewer 并存，viewer 的 `resume_manual` 在脚本占用期间拒绝，释放后仅恢复资格、不自动 claim 或发送手机动作；（3）worker 清理未知时保持 `cleanup_pending`，第二次请求不得把失败变成功；（4）仅关闭观看会话不释放属于脚本 run 的写锁；（5）外部 holder 占用时首次 view_only 不能夺取写权限；（6）错误 executor ID 不能降级或恢复手控。
- 该批测试**没有修改** `Journal.claim`、`Runtime.view_only` 或实际锁模型，避免以测试为由放宽互斥条件。现有 T17 测试文件继续保留。
- 尚待 Rust 主服务对零步正式脚本可保存/运行拒绝、CAS 409/账号越权、`viewer_only` 转换前后数据库状态的回归，且待执行器模拟用例和 FnOS/手机真实测试。**未执行** Python unittest、Rust 编译/测试、CI 手动触发、设备/浏览器验证。
## 2026-10-28 · T49 第二批（Rust AutomationStore + 真实资产 API）

- 创建 `easytier-web/src/db/automation/tests.rs` 并在 `mod.rs` 通过 `#[cfg(test)] mod tests` 接入测试树。
- 覆盖真实 `Db::memory_db` 迁移链中的：空正式脚本允许保存、可执行步骤必须非空、正式脚本更新 revision CAS 409、版本化安全删除 CAS 409/422 与正确修订删除、其他 owner 的脚本列举/更新/删除隔离、非管理员 actor 越权 403、不合法步骤拒绝且不改旧记录、外国账号不能创建执行占位，以及同一请求 UUID 不同正文的幂等冲突。
- 进一步复用 `AssetsStore`、真实网络配置/心跳/phone 语义类型/TCP ADB 服务 API，在纯内存库生成符合授权与 `target_fingerprint` 规则的**虚拟测试手机**，不连接物理设备。对实际 `create_run` 测空脚本 422 + 零记录、相同 UUID 重放不二次建任务、不同正文 409、旧修订 409 而新修订可建任务。
- 验证主服务 `begin_session_permission/confirm_session_permission` 二阶段写锁：`active` 与 `suspending` 期间不许运行；只有确认 `view_only` 后脚本才可录入 `dispatch_pending`；存在未清理 run 时不得 `resume_manual` 或建立第二视频会话，且旧观看行保留。
- 测试 ID / 函数 / 预期结果已汇总至 [T49 测试矩阵](task-49-test-matrix.md) RS01～RS12、EX01～EX06；其中真实设备验证属于 T50，不是本任务的自动通过证据。
- **未执行** Rust 编译/单测、Python unittest、前端类型检查、Vite、CI 手动触发、FnOS/Android/多浏览器验证。任何 SQL、迁移或模拟数据适配问题必须在获得测试授权后以真实日志确认；当前只能报告测试源码已提交，不打勾“通过标准”。
## 2026-10-28 · T49 第三批：有据可查的构建阻塞修复
- 从 GHCR 自动运行 [#40616428698](https://github.com/CiBeDerORG/EasyTier/actions/runs/40616428698) 发现 Rust T49 测试缺 `easytier_core::management::remote_client::Storage` trait 导入导致 `insert_or_update_user_network_config` 方法不可见；已由提交 `bbc6fe7b613bd6222d3bba8d7f7039362cb48b89` 修复。
- 该流水线并没有完成 T49 回归，属于**失败证据**。最新代码的 Rust 12 例和 Executor 6 例后续只能以新 Actions 明确成功才标通过；当前不凭推送和修复声明所有用例通过。

## 2026-10-28 · 接手后实际测试结果
- Rust 自动 CI [#40622528076](https://github.com/CiBeDerORG/EasyTier/actions/runs/40622528076)，源码 `9120b252`，`test_native` **50/50 通过**，其中 T49 12/12 通过。旧失败 [#40621494291](https://github.com/CiBeDerORG/EasyTier/actions/runs/40621494291) 为虚拟端点未模拟 cleanup confirmed，已在内存测试夹具修复，未改生产清理与写锁保护。
- Python Executor 自动 CI [#40615238092](https://github.com/CiBeDerORG/EasyTier/actions/runs/40615238092)，源码 `59bce21e`，Python **95/95 通过**，包括本模块 6/6；执行器已推送分支镜像。
- 通过的是纯数据库与模拟媒体安全用例；FnOS、Android 真机、真实 WebRTC 媒体链路、双浏览器操作租约和实际部署验证一律**未执行**。
