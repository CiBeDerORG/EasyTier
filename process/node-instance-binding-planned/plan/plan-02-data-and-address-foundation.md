---
id: P02
status: completed
depends_on: [P01]
tasks: [T02, T03, T04]
updated_at: 2026-10-23
---

# P02 数据、实例读取与地址基础

**最新状态：** 本计划已完成对应开发、授权验证及当前云环境交付；实际结果和未覆盖项见[验证报告](../task/verification-report.md)与[交付记录](../task/release-record.md)。以下验收设计保留，不将未实测范围扩大为已通过。

目标：为前后端提供可追溯的实例身份、精简配置与IPv4规则计算能力，保留旧资产数据和原网络功能。依据[后端需求](../require/require-02-backend-binding-and-addresses.md)、[生命周期需求](../require/require-04-lifecycle-and-compatibility.md)及[v2契约](contracts/v2.md)。源码已接入并完成授权验证，范围见[验证报告](../task/verification-report.md)。

## 当前基线

- `asset_gateway_groups/asset_devices` 分表且无实例绑定；原设备IP和组网段是手工资料。
- `db/mod.rs::list_network_configs` 可按owner/node读取持久配置All；旧在线实例列表不适合作为唯一资产候选来源。
- Core的proxy parser及CIDR转换支持原址代理与等前缀映射；资产地址预览尚无推导能力。

## 小任务与并行边界

|任务|唯一负责人|独占范围|输出|
|---|---|---|---|
|[T02 数据及旧资料](../task/task-02-schema-and-legacy-data.md)|schema-agent|新迁移、必要实体和实体/迁移注册|可空旧绑定、唯一约束、身份/快照存储、迁移说明|
|[T03 实例目录](../task/task-03-instance-catalog.md)|instance-catalog-agent|新 `db/assets/instances.rs`|受scope约束的持久配置读取、精简候选、IP来源与不可选原因|
|[T04 IPv4推导](../task/task-04-ipv4-rule-derivation.md)|ipv4-agent|新 `db/assets/address_derivation.rs`|规范化规则、稳定key、host offset及确定性地址结果|

T01先固定DTO、存储字段与helper签名；T02、T03、T04随后可并行。T03通过T01接口调用T04而不重写算法；未接入前可用声明/示例交接。三者不直接修改资产CRUD、HTTP、前端或旧网络写路径。公共模块注册由T12统一，T02仅负责实体和迁移注册。

## 完成定义与交接

- T02交T05/T06：结构、索引、空值与旧资料识别规则、事务读取方式；不自动猜绑定或重算旧地址。
- T03交T05/T06/T07：owner/node/实例作用域API、持久行身份、精简快照、IPv4来源、规则/配置状态；不得输出网络凭据。
- T04交T03/T05/T06及前端契约：纯函数输入输出、原址/映射和非/24例子、错误字段及规则指纹规范。
- 下游能在没有RPC在线依赖、没有后台任务的条件下完成预览与保存；重复ID跨节点合法、同节点网关绑定唯一。

后续授权验收范围：增量迁移保留旧记录/服务/IPv6、候选离线和管理员scope、仅运行态及配置异常、规则规范化与/31/32、范围越界与溢出。测试由T13集中安排，不在文档或每次实现修改后自动执行；本计划目前没有测试结果。
