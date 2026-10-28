# T02-06 — 外部删除安全判定与证据隔离

> 2026-10-28；基准分支 `feat/automation-center`。
> 当前状态：**IN_PROGRESS：连续完整扫描缺失证据与安全 UI 的 Rust/Web/镜像 CI #40662950752 已通过；M4 的排他资源归属/实际服务及公网映射清理暂未实现，不得虚报“外部删除自动清理已完成”**。

## 完成的代码

- 新增 `m20261028_000043_agent_docker_absences.rs`，按现有 `asset_device_id + agent_id + resource_id` 建立资源缺失证据，保存首次缺失时间、上次确实存在的完整 Docker 资源、最后存在时间、连续完整扫描缺失次数。
- `docker_absence::reconcile_and_cache` 在**同一个 SQLite 事务**里读取上一次完整快照、计算缺失/恢复、更新缺失证据和保存本次成功快照。防止“缺失计数更新了但快照写入失败”这种非原子状态。
- `asset_agent::Manager` 的 Docker 手动刷新增加互斥锁，避免两个并发刷新从同一历史快照重复计数。仍然没有后台巡检或自动轮询。
- 第一次权威缺失为 `needs_recheck`，连续第二次完整扫描仍缺失为 `confirmed_missing`。**confirmed_missing 仅表示 Docker 资源缺席得到两次验证，不代表允许删除关联业务数据。**
- 资源重新出现在完整扫描里时清除缺失证据；`exited`、`created` 或其他有实体的容器照样算“存在”；Compose 项目只要仍在完整清单里就不是已删除。
- 离线、查询失败、`complete=false`、Agent UUID 与绑定不一致、响应重复 ID 或缺少资源信息：**不写入缺失证据、不覆盖成功快照、不清理应用、服务或公网映射**。
- Agent UUID 更换时旧 Agent 的消失证据不得继承；只在新 Agent 完整扫描形成合法基线后再产生判断。
- 后端返回 `external_absences`，每项都有 `cleanup_allowed:false` 及其缺少排他所有权证明的阻断原因。
- `ServerAgentDockerPanel.vue` 只在实时完整查询成功时显示这些缺失证据，标明“首次缺失/连续缺失”，没有“一键清理”或自动删除行为。

## 真实自动清理的剩余门槛

目前 M3 可信 Deploy manifest、M4 Agent 受管实例登记及服务所有权关联都尚未完成，**不可以**因为容器 ID 或 Compose 项目名称和现有服务名称接近而删除 `asset_services` 或 `asset_service_publications`。目前的 `asset_agent_adoption_intents` 仅是等待验证的用户意向，既不是正式受管应用，也没有服务/公网转发的排他归属。

后续 M4/M5 应先增加明确的受管实例 UUID / 服务 ID / 公网映射所有权关联（有外键和主机作用域），再实现**仅匹配这些专属关联**的清理事务。清理执行前必须额外要求：

1. 当前设备/Agent UUID 与已登记的受管实例身份完全一致；
2. 连续两次不同按需完整 Docker 清单确认资源不在，且所有关联服务/公网映射从未被人工重新绑定或被其他应用复用；
3. 重新获取当前资源清单，并在同一锁/事务内核查归属和资源代际；未知状态或任意一点失败就保持记录，不进行清理；
4. 只清理排他受管记录、专属的服务与公网映射；用户自建服务、独立外部 Docker、历史数据/卷、其他网络实例的端口映射一律保留；
5. 端口映射必须用既有发布同步机制落实外部运行配置的删除，不能只删数据库就说已取消公网映射；无法确认远端同步成功时保持异常状态与可重试记录；
6. 不自动回滚，失败保留现场并展示错误。

## 测试

- [x] 已提交单元测试，证明 `complete=false`、非同一 Agent、重复资源 ID 不构成完整扫描证据。
- [x] 已提交 SQLite 迁移 + 数据库回归测试，覆盖首轮缺失、第二次确认、被停止的旧容器保留、资源恢复撤销误报、Agent UUID 更换隔离。
- [x] Rust 编译/部分已有原生回归与 Vue 构建：[EasyTier CI #40662950752](https://github.com/CiBeDerORG/EasyTier/actions/runs/40662950752) 通过。T02-07 已将新的 `docker_absence` / `docker_discovery_regression` 套件纳入 CI 必跑集合，最新结果待核验。
- [ ] 真实 FnOS 上 Compose、Docker daemon 异常、外部容器删除/恢复及端口发布回归，归 T02-07。
- [ ] 真正清理属于当前 T02-06 的未完成验收项，并与 M4 受管实例登记、M5 服务生命周期收尾；不能把 T02-06 标为 DONE。

## 下次任务

先按全局规则检查本轮最新 EasyTier CI，失败优先修复。若成功或执行中，继续 T02-07 的可自动化部分，以及 M3 的 manifest 校验准备。真正服务/公网映射清理按上述归属前置条件再完成。
