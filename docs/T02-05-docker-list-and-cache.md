# T02-05 — Docker 列表页面与按需状态缓存

> 日期：2026-10-28；基准分支 `feat/automation-center`。
> 当前状态：**代码/交互已提交，CI 最终结果下次执行任务前核查；FnOS 真实联调待 T02-07**。

## 实现交付

### EasyTier 后端

- Migration `m20261028_000042_agent_docker_snapshots.rs`：按现有 `asset_devices.id` 唯一关联 Docker 快照及 Agent UUID、采集时间，删除资产时级联清除。不是另外新建一套主机资产表。
- `asset_agent::Manager::docker_inventory` 完全按需：每次用户打开设备 Agent 面板或手动刷新，使用已绑定的 SPKI pin、密文 Bearer、Controller ID 和 Agent UUID 请求 Docker。
- 只有 Agent 身份验证通过、`status=ok`、`complete=true` 且资源结构有效时，才把新快照写入 SQLite；不会在 Engine 查询失败后写入空列表覆盖上次成功清单。
- Agent 网络/TLS 故障时，从 DB 读取 **相同 Agent UUID** 的上次成功结果，返回 `live_status=offline`、`fresh=false`、`complete=false`、`snapshot`；如果没有历史成功清单就返回空历史。
- Agent 存活但 Docker 查询不完整时，返回 `live_status=degraded` 和历史数据；不使用本次部分结果对历史做删除/覆盖。
- Agent 认证失效、绑定 UUID 不符等安全错误不能静默改成历史数据；仍抛出错误。
- 新的 `snapshot` 字段在实时返回中也提供标准化数据，且与顶层 `resources` 保持兼容。
- 不做后台轮询、无限重试、自动下架或自动接管，所有资源操作继续由 M4 持久化任务能力承接。

### EasyTier 前端

- 新增 `ServerAgentDockerPanel.vue`，挂到既有 `ServerAgentPairingPanel.vue` 的服务器 Agent 入口。
- 展示 Compose 分组、独立容器、项目成员、每容器镜像与状态、映射端口、上次采集时间，并区分 live/离线历史。
- 空容器列表与“未读取过历史数据”明确区分；停止中的容器/Compose 项目不会视为删除。
- 仅在本次完整扫描成功且用户明确勾选确认时，才可提交接管意向；并再次由 Rust 后端查询真实 Agent 防止过期成员、竞争条件。
- 已保存意向显示 `adoption_pending_catalog_validation`，成员变化显示 `adoption_needs_revalidation`，支持显式取消意向；**不显示安装、启动、停止、卸载按钮**，也不将意向当作真正受管应用。
- 前端异步响应通过请求序号控制，关闭/切换服务器时旧响应不会覆盖新设备状态；从页面离线恢复可手动刷新。
- TS `node-assets` 添加 Docker 资源/状态/接管类型，API client 按现有 REST 路径读写，无浏览器直接访问 Agent 或暴露 Bearer。

## 验收清单

- [x] Go T02-03 修复后 CI #40658529834 成功。
- [x] 数据库缓存和前端交互代码已提交；Rust/Vue CI **待下一轮开始前检查**。
- [ ] 验证实际 FnOS 上运行、停止、部分运行的 Compose 容器按需发现及离线恢复（T02-07）。
- [ ] T02-04 的**可信 Deploy catalog 匹配 + Agent 实际受管登记**未完成，继续保持进行中，后续 M3/M4 完成后才能验收。
- [ ] T02-06 完成删除判定：仅在不同于离线/不完整的**连续、完整且权威**快照基础上做外部删除确认，且严格限定服务与映射归属；本轮未增加自动清理。

## 本轮注意事项

- 禁止在 `fresh=false` 情况下发起接管；历史资源只读，`complete=false` 永远不能成为“缺失资源”证据。
- 无需等待 GitHub Actions 最终产出再结束本轮开发；下一轮先检查当前 commit 对应的最新 run 并优先修复失败。
