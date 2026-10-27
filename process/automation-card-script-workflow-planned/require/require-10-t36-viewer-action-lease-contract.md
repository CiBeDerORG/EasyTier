# T36 · Viewer / Action Lease 跨层状态合同（2026-10-28）

> 基线：`feat/automation-center`。此文件冻结目标协议，不代表 Rust/Executor/前端已全部实现。T37 / T38 / T39 必须分别核对源码后逐步接线；不能单独放宽 run admission。

## 1. 权属与不可变身份

- 一台 ADB 物理设备的权威写租约 key 为 `device_id OR endpoint(target)`（二者任一冲突都视作同机），由 Executor `Journal.controls` 在 `BEGIN IMMEDIATE` 内仲裁；保存 `target_fingerprint` 与具体 `holder`。
- **viewer（只读媒体会话）**：现有 `automation_sessions.id` 和 Executor `media._sessions[id]`，绑定 `actor_user_id + owner_user_id + device_id + target_fingerprint + executor_identity + session id`，只赋予观看现有 `PhoneCard.primary-video` 的权限。不因 viewer 存在推断其具有手控权限；一个设备沿用已有单主画面媒体流，不创建第二播放器或替代 WebRTC。
- **action（独占设备动作）**：`session:{session_id}`（manual）、`prepare:{preparation_id}`（prepare）、`run:{run_id}:{device_id}`（script）。任何瞬间每一设备/endpoint 最多一个持有者；观看者可与 prepare/script 共存，但不可写。
- 每次动作写入须以当前目标指纹和 Executor identity 复核；不可只相信前端按钮灰色、缓存 device 列表或过期快照。既有 request_id/command_id 的幂等语义保持；失败或结果未知不自动重试实际动作。
- 旧 Executor/Rust 无新协议能力时 **拒绝只读移交**，沿用原先“先停预览、确认清理后运行”模式，不允许新前端假定老执行器有分权功能。

## 2. 状态、转换与可观测信息

| 会话权限 | 视频 | 输入动作 | 转换要求 |
|---|---|---|---|
| `active`（旧兼容的 manual 模式） | 授权可看 | 必须通过 journal claim | 只能原会话 actor 申请转只读 |
| `suspending` | 可继续看 | 立即拒绝新手动写 | Rust 先持久化拒写，然后执行器停手、等待旧动作结束，且确认 release |
| `view_only` | 授权可看 | 全部拒绝（包括 stop / touch.release / unlock / text） | Executor 已确认手动 holder 清理；Rust 此时才允许 run 创建忽略此同机 viewer |
| `resuming` | 授权可看 | 仍拒绝 | 只在无 pending run、无 cleanup_unknown 的同一个事务内完成 |
| `active`（恢复） | 授权可看 | 再次允许按需竞争 action claim | 不隐式抢占 script 或 prepare |
| `cleanup_pending` / `outcome_unknown` / `revoked` | 仅能在媒体仍可用且账号有效时看 | 全部拒绝 | 必须显式确认回收，不能自动释放锁、重开会话或重发设备指令 |
| `closed` / `failed` | 不可看 | 不可写 | 会话 ID 不可复用 |

`viewer` 并非新的 ADB 服务，也不需要另一条 WebRTC。执行器的媒资状态、Rust session 权限与 Journal action 锁是三个独立事实；前端必须分别显示，禁止将“画面可见”当作“允许输入”。

## 3. T37 → T38 → T39 协议交接

1. **T37 / Executor**：先修正 `Journal.claim` 重入条件，旧 holder 的 `kind / target_fingerprint` 必须匹配；`stop/touch.release` 不可在脚本或其它 holder 占用时绕过独占校验。新增显式、经 `expected_executor_id` 校验的 `sessions/{sid}/view-only`：在 session 命令锁下完成取消、已发送触摸释放与 worker 清理；确认无该 session action holder 才回 `state=view_only`，保持 media 与 WebRTC SDP 原样。所有 session 写入口同时检查只读标志；已关闭/重启/绑定变化拒绝幂等假成功。
2. **T38 / Rust**：新增具有过渡状态及 CAS 的 session 权限迁移和只读许可；`create_run` 的 `BEGIN IMMEDIATE` 原子准入仅忽略**已确认 view_only、目标指纹一致、同 owner 同设备**的 viewing session，其他任何 session、suspending、cleanup_pending 均保持原阻挡。WebSocket ping / session 续期允许 viewer，所有 command、unlock 必须额外检查当前权威动作许可（不能只依赖建立 WebSocket 时的权限）。执行完成后的 manual 恢复只能在无运行/清理未知的事务内完成。若跨进程迁移结果未确认，维持拒写，不声称成功。
3. **T39 / 前端**：`PhoneCard` 的主视频和 `sessionId` 均保持；发起试跑先申请 viewer 降权、再通过原 `POST /runs` 创建运行，拒绝通过 `releaseOwnedTrialPreviews` 直接销毁已有主画面。只有服务端已确认 `active` 才允许触屏/解锁/输入/键盘/群控；本地剩余 pointer 与队列在降权时清空。运行完成/清理确认后显式恢复；断线或状态未知保持只读、提示重新确认。
4. **不允许先行替换的业务路径**：旧 `CardTrialPanel`、`ScriptLibrary`、`AutomationCenter.releaseRunPreviews` 均有历史安全准入。两端协议接齐并测试之前，不得直接移除预览回收；版本不支持时提示安全阻断。

## 4. 竞态与错误合同

| 场景 | 必须行为 |
|---|---|
| 两浏览器同机查看/写 | 现有单主媒体会话拒绝第二个观看会话；可共享授权只读观察但不得另造控制会话。持有者以服务端记录为准 |
| 旧会话正在触摸 / 输入文字 | viewer 申请必须等动作锁完成，确认释放；超时/取消/异常标记不可写，不可开放脚本 |
| run 创建与 viewer 恢复竞争 | 两者在同一 SQLite `BEGIN IMMEDIATE` 保证线性化；最后一个成功授予相应权限，其它操作返回 device_busy |
| 设备重新绑定 / fingerprint 漂移 | 双边拒绝新写；旧 media 不得自动指向新 ADB 地址 |
| 断网 / 响应超时 / Executor 重启 | 不凭本地计时释放写锁；未知结果保留 `cleanup_pending`/锁，必须人工确认 |
| 取消脚本 / 前端关闭浮窗 / 全屏 | 前者按原 run journal 清理，后两者均不影响 action lease，也不重新开视频 |
| 视频丢帧或失败 | 准确显示媒体异常，脚本按既有授权与执行事件决定状态；不能假称屏幕仍在播放 |

预期错误码：`device_busy`、`target_changed`、`session_revoked`、`action_lease_required`、`viewer_only`、`cleanup_pending`、`outcome_unknown`、`executor_identity_changed`、`lease_transition_unknown`。必须经 Rust/Executor HTTP 响应保持明确、不得把 5xx 一律转成“已释放”。

## 5. 拒绝降级的验收线

- 同一手机“手控 → 只看+准备 → 只看+脚本 → 只看+清理 → 手控”过程中唯一主视频 sessionId 不变，脚本修改屏幕可在其上看见。
- 在 run 进行中对同机发送 tap、key、input_text、unlock、stop、touch.release 均在后端拒绝；另一浏览器不因另一人关闭窗口继承写权限。
- 测试两种投屏分辨率、手机旋转、两个浏览器、Executor 故障重启、过期授权、adb 重绑定与清理未知；所有模拟通过也不等于 FnOS+Android 真机通过。
- 迁移不是一次性切换开关：当前交付只有本文件和另行登记的实际代码提交；未在调试模式擅自执行构建/测试/部署。

## 6. 已有代码基线与执行顺序

- `runtime.py::command` 的 `stop/touch.release` 过去未检查当前 journal action holder，是 T37 优先修补的独立危险入口。
- `Journal.claim` 同 holder 重入过去只核验 active，不复核 kind/fingerprint，作为 T37 防线增强。
- `db/automation/runs.rs::create_run` 目前无差别阻止所有活动 session；在真分权协议完成前这一安全门禁应保持。
- `restful/automation/sessions.rs::control_loop` 当前在 WebSocket 退出时关闭会话；未来 viewer 仍须拥有已有保活信道，但输入按只读权限另行拒绝。
- `automation-run-handoff.ts` 当前会关闭试跑预览，这是旧准入合同；新路径须在端到端分权完成后迁移而不是直接删掉。
