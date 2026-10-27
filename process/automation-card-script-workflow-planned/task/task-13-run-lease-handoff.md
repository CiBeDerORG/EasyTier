# T13 · 试跑预览移交与独占二次防护

- 里程碑：M4；负责人（唯一写入）：执行器/服务端 Agent；相对工作量：L
- 依赖：T12；映射验收：AC-09
- 当前状态：**代码已提交，待联调**；未运行本地测试、构建或 Android/FnOS 真机验收。
- 写入范围：`easytier-web/src/db/automation/mod.rs`、新增 `frontend/src/modules/automation-run-handoff.ts`、`frontend/src/components/AutomationCenter.vue`（最小接线）；`runs.rs` 和执行器运行排他已满足本任务基础约束，暂未修改。

## 交付范围
核验已有 create_run 排他/cleanup，尽量复用；提交前关闭本页预览、确认关闭并二次鉴权；其他会话不强占，异常不静默重放。

## 完成定义（DoD）
真实占用仍拒绝，自己预览可安全交接；失败不发脚本；并发用户不能同时取得控制权。

## 测试与交接
- 完成后记录变更文件、关键接口、异常分支、真实执行的测试与缺失测试。
- 交接给集成负责人验证与原卡片/脚本库闭环；不擅自修改其他 Agent 的唯一负责文件。
- 若实现依赖和实际源码不符，先更新任务卡并解释原因，不得绕过权限或设备独占协议。

## 2026-10-27 · 已提交的实现与保护边界
- 服务端反向资源栅栏：`AutomationStore::create_session()` 在 `BEGIN IMMEDIATE` 事务内按设备 ID / ADB target_key 检查尚未确认清理的运行设备，与原 `create_run()` 的预览会话栅栏相互对称。两种创建操作不再存在“脚本已入库但浏览器抢先开预览”的数据库准入空档。
- 前端 `automation-run-handoff.ts`：限定运行对话框明确跟踪的本页预览（`scriptPreviewIds`）；先读取账号设备目录校验远端 active_session 的 ID 是否就是卡片当前所持会话，禁止关闭其他人的会话。
- 关闭本地卡片会话后，要求没有本地 session ID、没有 `cleanup_pending`，并再次通过服务器 `GET /devices` 确认 active_session / run_occupied 均不存在；否则中止整个提交并展示错误。未关闭的预览保留，不擅自关闭其他设备。
- 复用 `ScriptLibrary.submitRun()` 现有二次指纹/绑定读取和幂等 `request_id`，以及 `create_run()` 的事务检查，未知 HTTP 结果沿用相同请求 ID，不触发新动作。T14 卡片新入口必须走这一安全通道，不得绕过。
- Python Executor `Runtime._run_device()` 已通过 `journal.claim(target,holder,"script")` 占用与 `_authorize` 授权后才执行；`create_session()` / `journal.claim(...,"manual")` 采用同一控制表。暂不改 Python，避免引入多套锁协议。
- 不保证业务设备永远空闲：前端复核是提示，最终以 Rust 事务与执行器独占为准；客户端网络结果未知、设备变化或多浏览器并发，必须拒绝/提示，不自动重放副作用。

## 未完成的实测门禁
- 多浏览器同时发起“试跑/投屏”、与清理中任务重叠：只允许一个控制权持有者。
- 手机卡片自己关闭成功、清理待确认、HTTP 断线、异步刷新后旧结果返回；不可误判为释放。
- 多台手机：某台无法释放时不得派发本次脚本，也不得关闭非本页预览。
- 对应版本 Rust 编译、Vue 类型检查、Web CI、FnOS+Android 真机及多标签页实测 **均未执行/未确认**；遵循调试模式，仅允许 push 自动触发已有 CI。
