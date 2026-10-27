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
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。
