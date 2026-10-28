# P14 · 全局完整流程执行与多手机结果

> **状态：T43～T45主链路已提交、待前后端联调与真机验收**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
当前脚本主浮窗一键将不可变流程发给工作台已勾选手机，设备间并行、设备内顺序，观察各手机原卡片视频并回溯结果。

## 前置条件与关联任务
- 前置：P09 + P12；界面 P08。
- 开发任务：[T43](../task/task-43-batch-submission.md)、[T44](../task/task-44-batch-progress.md)、[T45](../task/task-45-batch-original-video.md)。

## 交付要求
1. 仅勾选且授权设备进入 batch；0 台无入口或明确禁用，不默认全账号设备。
2. 明确正式 revision/草稿快照、run request UUID 与目标清单冻结；当前用户后续切脚本不影响在途运行。
3. 复用现有运行队列、并发、日志、历史/修复；按设备、按步骤展示实进度，失败设备不拖累其它。
4. 不建立 ScriptLibrary previewPlayer；A 使用 A 卡片原视频、B 使用 B 卡片原视频。
5. 停止某台/停止全部为请求取消，不能在未知结果时发第二遍命令；推荐并发不是简单固定 UI 20 台上限（应与后端容量相匹配）。

## 可验收判据
- [ ] 两台手机不同进度与结果互不影响；对应视频可实时观察，不被创建运行时关闭。
- [ ] RunHistory 与浮窗展示 run_id/device_id/step_id 一致，未知结果未自动重放。

## 边界与风险
脚本全局入口和单卡片 ▶ 目标来源必须分开；不抢其它浏览器投屏，owner 授权逐设备校验。

## 禁止提前声称完成
已有阶段性业务代码提交；无真实验收证据时状态仍为“待联调/待验收”，不把文件创建计作交付。
## 2026-10-28 · P14 实际交付与剩余风险
- 新编排 `useFloatingBatchRun.ts`：仅使用工作台勾选手机和当前已确认服务端同步草稿，冻结 device+fingerprint+steps+params+一次 UUID；逐机借用各 PhoneCard 的唯一原 WebRTC，会话降 `view_only`，独立准备 ADB/UiAutomator，所有设备核对成功才批量 `POST /runs` 一次。
- `FloatingBatchRunPanel.vue` 与 `AutomationCenter`：同一脚本主浮窗显示逐机排队/运行/步骤/完成/失败/未知及原视频的正常/断流状态。账号/工作台页面切换、筛选、设备投屏关闭受到进行中 batch 保护；服务端写锁仍为最终判定。
- 每机停止复用 Executor 现有 `runs/{run}/devices/{device}/withdraw`，Rust 新建外部 `POST /runs/{id}/devices/{device_id}/cancel`，先持久化撤回意图，再核验 Executor fencing、run结果与 cleanup；其它设备继续跑。全停沿用旧取消接口。HTTP 结果未知不自动重发动作或重复停止请求。
- 并发无固定执行器上限：旧 `automation_runs.concurrency` 列 CHECK 1～3，新增迁移 `m20261028_000028_automation_run_concurrency.rs` 用从表保存 >3 的实际配置，创建时旧列保持兼容，读取/派发优先使用扩展值；不重建历史 run 外键表。
- 仍有独立的**单次 20 台设备 API 硬限制**，新前端如实展示/拦截，不自动拆成多次提交。若要单批 >20 台，需跨 Rust、Executor 和结果处理协同扩大上限，不能只改 UI 数字。
- **旧兼容边界**：`ScriptLibrary` 中的历史 previewPlayer / releaseRunPreviews 尚保留，T46 安全替换完成后才按基线删除；新批量入口不走旧分屏。
- **尚无运行证据**：未运行编译、TS/Python/Rust 检查、单测、CI、FnOS/Android 真机、双浏览器争用及数据迁移校验。P14 验收框全部未勾选。
