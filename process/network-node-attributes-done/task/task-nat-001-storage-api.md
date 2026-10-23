---
id: NAT-001
feature: network-node-attributes
status: done
owner: /root/node_attributes_backend
depends_on: []
parallel_group: implementation-contract-v1
plan: ../plan/plan-nat-001-storage-api.md
requirements: [NAT-01, NAT-02, NAT-03]
acceptance: [NAA-01, NAA-02, NAA-04]
updated_at: 2026-10-22
---

# NAT-001 属性存储与 API

[任务看板](README.md) · [实施计划](../plan/plan-nat-001-storage-api.md)

## 独占写入范围

- `easytier-web/src/db/devices.rs`
- `easytier-web/src/db/entity/devices.rs`
- `easytier-web/src/db/mod.rs`
- `easytier-web/src/migrator/mod.rs`
- `easytier-web/src/migrator/m20261022_000011_node_attributes.rs`
- `easytier-web/src/restful/network.rs`

只读前端和 `client_manager` 以核对接口；不修改这些文件。DB 中接收 IP 观察的结构和函数由本任务实现，与 root 提前同步函数签名。

## 工作项

- [x] 新增八列迁移、实体映射及必要导出，保留 main 现有迁移序列和旧设备数据。
- [x] 实现自动 IP 快照持久化及两个手填字段独立原子更新，心跳不覆盖手填值。
- [x] 扩展机器列表；实现仅更新已存在本人设备的 PUT，拒绝外部 webhook 模式和未知字段。
- [x] 实现两字段必填字符串、控制字符拒绝、长度限制、空白修剪及空串清空。
- [x] 补充迁移、数据隔离、缺失节点、原子性、错误参数及重连写入保留属性的针对性测试。

## 交付与门禁

向前端交付契约一致的 JSON，向 root 交付八列迁移和 IP 存储函数。交接前确认不含用户管理分支迁移或特有测试身份结构，不新增定时任务。

验证由 root 协调统一执行 Rust 检查，避免并行争用构建目录。至少覆盖迁移测试、`manual_attributes_*`、`machine_attributes_*` 和 IP 持久化场景；记录实际命令和结果后才勾选验收。源码中已有测试名称不代表已执行通过。

## 当前进度与问题

状态：已完成。`cargo test --locked -p easytier-web` 为 294 通过、2 项原有大规模连接用例跳过，覆盖本任务数据库/API/迁移检查；另经独立只读审核未发现问题。联调也已完成，见[验收记录](../verification.md)。
