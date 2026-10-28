# T44 · 并行设备结果、停止与失败修复入口

> 状态：**逐机进度、全停/单机撤回和可配置并发已接线，未联调验收**｜计划：P14｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**运行/历史 Agent**。
- 文件边界：RunHistory.vue、useCardRunStatus.ts、浮窗 run 状态组件。
- 开始条件：T43、T41。

## 实现步骤
1. 设备间并发、设备内步骤按序，按设备展示排队、运行、完成、失败、取消与未知。
2. 浮窗可以单独停止某设备或整体请求停止，严格区分请求取消、已确认停止和 cleanup_pending。
3. 连接历史不可变快照与失败修复，回到对应手机浮窗编辑；核查并发推荐上限与旧 UI 固定 20 台拦截一致性。

## 通过标准
- [ ] A/B 不同设备进度不互相阻塞，历史能按 run/device/step 追溯。
- [ ] 停止/超时/未知不能自动重放剩余或失败手机动作。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
运行结果不能只存前端临时状态，必须复用服务器事实回执。

## 本任务当前状态
已提交代码，仍未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T44 代码交付
- `useFloatingBatchRun` 仅 GET 查询同 runId 的各设备 state、当前 step 和持久运行记录；按设备独立显示排队、运行、完成、失败、cancel_requested/outcome_unknown/cleanup_pending。不根据前端推断清理成功、不重放 POST。
- 新 `POST /runs/{id}/devices/{device_id}/cancel` 先在 Rust `BEGIN IMMEDIATE` 持久化 `withdraw_requested`、稳定 `withdraw_event_id`，再复用已存在的 Executor `withdraw_device` 独占回收协议，只针对本 run+device；执行器返回 `never_started` 证明或全量运行真结果才更新状态。其他设备不受该请求影响。
- 整批 `cancelRun` 仍调用原有授权 API。回执/网络未知只显示“已请求停止/结果未知”，绝不冒充“停止成功”，提供只读刷新。
- Executor `max_running` 已具备可配置能力，但旧 `automation_runs.concurrency` 表历史 CHECK 限制 1～3。本阶段新增 `m20261028_000028_automation_run_concurrency`：正向迁移扩展表保存大于 3 的并发数；原运行表保留兼容的 3，Rust `read_run/dispatch_payload` 使用扩展值；不重建已有外键/历史表，不引入固定最大并发。
- **状态：代码已提交，未测试/构建。特别需要验证单机停止过程中其它设备不取消、Executor 停止超时或重启、SQLite 迁移、旧数据、并发大于 3 的真实派发。**
