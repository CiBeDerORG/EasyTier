# P13 · 单步骤点击即执行与失败修复

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
主浮窗中每条可执行步骤点 ▶，只在该手机自动准备、校验、执行、反馈、修复，原实时画面持续可看。

## 前置条件与关联任务
- 前置：P10 + P12 + P11。
- 开发任务：[T40](../task/task-40-single-step-orchestrator.md)、[T41](../task/task-41-single-step-progress.md)、[T42](../task/task-42-single-step-repair.md)。

## 交付要求
1. 使用卡片 deviceId 锁定目标；无需再弹“自动准备试跑设备/确认执行此步骤”的普通流程；后台自动预检及必要参数输入。
2. 保留 owner、fingerprint、ADB、UIA、selector 唯一匹配、坐标宽高严格一致；硬错误阻断而非勾风险确认绕过。
3. 按 stepId 显示 preparing/running/action_sent/assert_pass/failed/outcome_unknown，freeze request UUID 和步快照。
4. 保留失败回步和 retry 显式人工操作、RunHistory 查阅；未知动作不自动重试，取消要求执行器确认。
5. 旧 CardTrialPanel modal 确认替代接通后移除其展示层，保留回执/安全逻辑。

## 可验收判据
- [ ] 同一卡片点一次 ▶ 只发一次指定步骤，不影响其他设备；原视频不停。
- [ ] 参数缺失才弹子浮窗输入，执行期间页面仍可观察，失败回编辑步骤并可再次显式试跑。

## 边界与风险
定位演示是只读 GET，不应混入真实执行状态机；需要防网络重发造成重复手机动作。

## 禁止提前声称完成
原需求计划已有阶段性代码交付；无实测验收证据时状态必须维持“待联调/待验收”，不把文件创建计作交付。

## 2026-10-28 · P13 代码阶段进度

- T40：原手机画面内的可拖单步浮窗已取代旧 body modal；点击 ▶ 自动准备，单次冻结 request UUID/step 快照，安全的只读检查准备通过可自动提交；真实动作或不确定定位需一次操作确认。
- T41：每步实时进度、原 runId、请求 UUID 已透传唯一 CardStepTimeline；不复制媒体会话。
- T42：已迁移原底部失败回修入口至同视频子浮窗，保存修订后才允许人工再试跑；真实闭环与旧历史跨Tab清理待验证。T41 耗时已支持按服务端 started_at/finished_at 投影。批量脚本原视频路径归属 P14/T43～T45，未因此间接宣布完成。
- **全部验收待完成**：未执行类型检查、单元测试、构建、浏览器、FnOS/Android 真机验证（调试模式）。

- T42 交接：`CardRepairEntry.vue` 只读比对历史失败快照，且必须当前步骤确有修改、共享草稿同步 guard 为 `clean` 才显示可执行重新试跑；`PhoneCard.vue` 浮窗与主视频同容器，unknown 顶层关闭必须保持人工确认保护。真实设备/双浏览器/执行器日志尚未验收。
