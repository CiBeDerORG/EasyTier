---
id: T13
plan: P07
status: verified
owner_role: backend-integration
depends_on: [T02, T03, T04, T05, T06, T07]
updated_at: 2026-10-23
---

# T13：后端统一注册与节点删除前置保护

## 当前状态（2026-10-23）

状态：**已验证（`verified`）**。

后端统一路由和删除前置保护已通过新增 DB/REST、真实 HTTP、旧库升级及真实 TCP config-server 注册验证。既有后端测试 294 项、新增资产测试 17 项通过；两项既有压力测试未执行。

验证依据：[2026-10-23 验证摘要、日志和截图](verification-2026-10-23.md)。已验证仅指报告列出的实际执行范围，不表示下方规划的完整场景矩阵全部测试完成。

本次文档同步未运行测试、构建或 CI；未来验证仍遵守调试模式和用户授权范围。

## 规划与联调前交接记录（历史）

> 以下保留最初计划、代码基线和集中验证前的实施交接。原文的“本轮未执行”“待授权验证”“尚未实施”等描述仅记录当时阶段；当前进度和实际结果以上方状态及验证摘要为准，不能将旧基线当作现在的代码状态。

联调前状态记录：后端模块、REST 路由与节点删除保护已写入并接入，待授权验证。来源：节点有下属数据禁删、Q31–Q39、Q46、F19；对应 [P07](../plan/plan-07-integration-and-ui-polish.md)。已接收 T02–T07 的模型/领域接口及接入说明。

### 独占修改范围

实际旧入口接入范围为 `easytier-web/src/db/mod.rs`、`db/devices.rs`、`central_network/service.rs`、`restful/mod.rs`。`db/assets/mod.rs` 随领域存储由 store owner 维护，`restful/assets/mod.rs` 由统一 REST owner 装配 T04–T07 后交接；不再按任务分配多个 REST 子模块。迁移/实体注册属于 T02，交接后顺序接入，不能并行争抢；不顺手修改 config-server/OIDC。

### 代码事实

`CentralNetworkService::delete_device` 原流程持有 `mutation_lock` 后发布配置并等待停止，之后才调用 `delete_device_with_optional_block`。本轮在运行副作用之前接入 `node_has_assets`，同时保留 DB 事务末尾兜底。创建组/设备复用 `lock_mutations()`，SQLite 写事务不能跨网络等待。旧删除还可能拉黑，资产设备仍使用独立资料删除接口。

### 实施步骤

1. 注册新 db/restful assets 模块与 `/api/v1/assets` 路由，复用既有 session/auth、db、central service 实例；新能力每个 handler 都执行 T04 的 owner/模式检查。
2. 集成资产专用 context/accounts/nodes/counts/metadata，不以当前在线 session 代替已登记节点，不以 `centralEnabled` 或旧未过滤权限结果判定管理员。受权节点删除入口显式传目标 owner 后调用旧中央删除服务，不误用仅从操作者取 user_id 的旧 handler。节点手工地点/型号沿用原字段与校验、自动 IP/地区只读；自己的旧属性接口继续可用。
3. 在 `delete_device` 已获取锁后、任何网络配置清空/停止/拉黑前调用 T04 下属资料检查：空组、非空组或直属设备均阻止删除；静态角色 metadata 不视作下属资产。
4. 让组/设备创建与上述删除持同一协调锁且顺序一致；只在调用边界取锁，禁止同链路再次取锁。最终删除事务/FK补防孤立数据，不依赖 UI 检查。
5. 旧 REST 删除将有资产的拒绝映射为明确冲突，保持原无资产删除语义；metadata 随合法节点删除清理，不删除其他账号资料。
6. 安装 T05–T07 路由、只读校验预览与统一错误码。组/类型/设备/服务写入都能调用同一幂等/校验底座，双协议和级联事务不能被装配拆开。
7. 形成给 T14 的实际接口清单、上下文样例和未解决问题；字段改动先协调 T01/T08，不在前后端各加临时兼容字段。

### 可审阅验收条件（尚未执行）

- 有下属资产的节点删除在发布配置前结束，实例/配置/拉黑状态不受影响；并发新增不能穿过检查产生孤立记录。
- 普通/管理员/外部认证模式三种路径都由后端决定能力，所有父对象归同一目标 owner 和 node。
- 新路由完整可接入；无重复模块名、无把既有权限组当网关组、无新增周期任务；旧 network revision 未改。
- 旧节点重复删除、未找到节点等错误按原语义或明确新冲突返回，不伪造成功；领域异常交回对应 owner。

本任务交付集成代码及静态交接记录。测试/构建/运行验证集中 T15 按授权执行；未执行时更新 test-report，不填写通过。

### 本轮实施交接

- `easytier-web/src/db/mod.rs` 注册资产模块；`restful/mod.rs` 合并资产路由。实际领域入口为 `db/assets/mod.rs::AssetsStore`，HTTP 入口为 `restful/assets/mod.rs::router`。
- `easytier-web/src/db/devices.rs` 新增 `node_has_assets` 及删除事务内兜底；`central_network/service.rs::delete_device` 在持锁后、发布配置/停止/拉黑前检查下属组和设备。组/设备创建 handler 使用同一协调锁。
- 旧节点无资产删除、有资产拒删、并发创建/删除、授权及模块装配等验收全部尚未执行；未运行测试、构建、应用或迁移，不能将静态接入描述为联调通过。
