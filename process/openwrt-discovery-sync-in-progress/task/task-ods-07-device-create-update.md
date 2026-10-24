---
id: ODS-T07
feature: openwrt-discovery-sync
title: 按预览执行新增与更新
owner: Codex
status: done
depends_on: [ODS-T02, ODS-T06, ODS-T15]
integration_dependencies: [ODS-T11, ODS-T17]
requirements: [ODS-R03, ODS-R08]
plan: PLAN-ODS-04
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T07 按预览执行新增与更新

所属：[ODS-R03](../require/require-ods-03-asset-apply-overwrite.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)；计划：[PLAN-ODS-04](../plan/plan-ods-04-device-apply.md)。文档作者不是实施负责人，本任务尚未认领。

## 输入与输出

输入为 T02 的字段/组级唯一约束，T06 的版本化预览和逐条用户选择，以及T15已修正的单设备失效触发器。输出逐条新增、更新、跳过结果与提交后地址变化描述；接口只遵循[公共契约](../plan/contracts/v1.md)。T11/T17仅为完整联调前提，不能以未联调为由跳过T15硬依赖。

## 独占拟改范围

- 既有 `easytier-web/src/db/assets/devices.rs`：`validate_relations`、`create_device`、`update_device`的组内 IPv4 检查；拟新增事务内设备构造/更新辅助。
- 拟新增 `easytier-web/src/db/discovery/apply.rs`：拟新增内部符号 `apply_reviewed_rows`、`apply_one_row`，负责预览应用与幂等重放，导出声明由T01统一冻结。
- 只读：T06 预览、`binding_ops.rs`、`address_derivation.rs`、`publications.rs`、T02 迁移。公共模块声明和路由交给 T01/T19。

## 小步骤

1. 先检查批内IP/目标重复争用，再重查账号、发起网关与当前组；目标限组内非网关自身，校验预览版本及旧设备，拒绝越组和陈旧选择，不按数组顺序覆盖冲突。
2. 同组占用 IP 禁止新建；未命中可手选旧设备，但不能抢占另一记录的 IP。更新保留 ID、手机类型、人工名称和未知字段原值。
3. 按当前网关规则派生地址，规则缺失/歧义返回该项待处理。以逐条事务保存设备和发现资料，复用既有 `sync_device_address` 更新期望公开配置，不在事务内访问远端。
4. 保留全部服务；记录幂等结果与变更描述。重复请求不重复创建，失败整体回滚该项，其他成功项独立报告。

## 完成标准与未来验证

覆盖 ODS-R03-A01～A04、A07～A08。未来验证同组竞争占 IP、跨组同 IP、手选目标、网关自身保护、空字段、手机类型保留、规则变化、事务失败及幂等重试。测试、构建和 CI 本轮均未执行，文档完成不代表这些场景通过。

## 交接

向 [T08](task-ods-08-device-overwrite-services.md) 提供可在同一事务使用的设备构造/校验适配；向 T19 提供逐条结果。运行映射由 [T11](task-ods-11-public-mapping-apply.md)/T19接线，不提前调用T11新符号；T17提交后处理目标变更，旧会话退出未确认前不能让新目标继续旧脚本。

ADB补采关联预览的应用成功或 `skip` 结果携带 `originating_operation_id`，经T19交T10恢复原ADB操作并结束相应待处理项。T07不重新扫描、不二次补采、不实现另一套reconcile；原ADB快照过期时由T10显式结束为待重新发现，只有用户新建扫描操作才取得新快照。

## 本轮实际交付

按行事务新建/更新，保留人工名和未知字段，组内唯一和幂等已验证。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
