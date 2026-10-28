# T02-04 — Docker 发现、匹配和接管（第一阶段）

> 2026-10-28。代码仓库 `CiBeDerORG/EasyTier`，分支 `feat/automation-center`。
> 状态：**IN_PROGRESS**（发现代理、待验证接管意向、去重与取消入口已提交；正式可信模板匹配、受管升级未完成，CI 最终结果下次开发启动时检查）。

## 当前可运行的能力

1. **仅在手工调用时查询**：`GET /api/v1/assets/devices/{id}/agent/docker/inventory?owner_user_id=...&machine_id=...`。每次重新执行已有 `server_agent_entry::load_authorized` 的当前登录用户、机器及服务器语义验证；Rust 后端从既有 `asset_agent_bindings` 取出密文 token、Agent UUID、pinned TLS SPKI，才向真实 Agent `/v1/docker/inventory` 发起 HTTPS 请求。
2. **Agent 身份一致性**：拒绝 Bearer 失效、TLS pin 不符、错误 Agent UUID、异常响应体；不会信任浏览器提供的 Agent URL 或 Token。Agent 探测成功但 `complete=false` 时，仍返回不可用于资源删除或接管的非权威清单。
3. **接管意向**：`POST /api/v1/assets/devices/{id}/agent/docker/adoption-intents`，body 示例：
   ```json
   {"resource_id":"compose:sub2api","application_id":"sub2api","expected_container_ids":["web123","db123"],"confirm":true}
   ```
   必须显式 `confirm:true`；前端传入的完整成员 ID 必须和 Agent **最新成功的完整扫描** 结果完全一致。身份不存在、容器成员发生变化、重复 ID、重复资源归属及单主机相同 app ID 均拒绝。先校验真实 Agent，再插入 SQLite 意向表。重复相同请求幂等地返回已有意向；尝试用不同 app 复用同一资源、或将同一 app 链接不同资源会冲突。
4. **无权限升级**：此阶段的意向记录是 `adoption_pending_catalog_validation` 而非真正的 `managed`；所有资源仍返回 `managed:false`。如果资源成员在下次扫描中变化，意向呈现 `adoption_needs_revalidation`，不得按旧快照继续升级。绝不根据容器名称、项目名称或镜像标签相似就授予完整受管权限。
5. **用户可撤销**：`DELETE /api/v1/assets/devices/{id}/agent/docker/adoption-intents/{resource_id}`，只删除该 owner + device + Agent 范围内的意向记录，不删除 Docker 容器、数据、Compose、服务或 Agent 注册记录。
6. **数据表**：迁移 `m20261028_000041_agent_adoption_intents.rs` 添加 `asset_agent_adoption_intents`，主键 `(asset_device_id,resource_id)`，唯一键 `(asset_device_id,application_id)`；按 Agent UUID 隔离旧绑定。前端所用的现有设备数据为唯一宿主机身份，不新增一套 Host 主数据。

## 跨模块依赖与未完成范围

- [x] Rust 后端 HTTPS 代理 + 会话鉴权 + 绑定 Agent UUID 验证。
- [x] 手工采集真实 Docker 资源，返回 `external` / `adoption_pending_catalog_validation` / `adoption_needs_revalidation` 分类。
- [x] 明确确认和最新 Docker 容器成员校验，SQLite 唯一约束防止重复意向；支持取消。
- [x] 输入 ID、重复成员、拒绝未确认请求的纯函数单元测试已纳入代码。
- [ ] **正式可信匹配**：需要 T03-01～03 先将 `Deploy/<id>/app.yaml` 变成经过 schema、跨字段、Compose 一致性验证的中央商店索引；现有例子只是 schema fixtures，不能作为已发布可信 manifest 进行自动接管。
- [ ] **实际受管升级**：必须在 M4 引入 Agent 本机持久受管实例登记、目录占用阻断、资源签名/归属校验，且只允许来自固定 catalog commit 的真实应用。此阶段不得先开放 install/update/configure/uninstall。
- [ ] **前端可视化与离线缓存**：T02-05 将添加列表、状态提示与手动确认 UI；离线结果必须标注陈旧、不能用于接管/资源删除。
- [ ] **真实 FnOS Docker 端到端**：T02-07 回归验收。
- [ ] **本轮 CI**：提交后触发主 Rust/Web 镜像构建；根据全局规则不等待，下次继续时检查和修复。

## 安全规则

- 禁止从浏览器直连 Agent；禁止把 controller Bearer 传给前端。
- 禁止把不完整 Docker 清单、离线缓存、历史容器成员用于清理或权限升级。
- 接管意向保留历史，Agent UUID 变化时不自动重用旧记录。
- 用户撤销意向只撤销权限意愿，不卸载原有外部应用。
