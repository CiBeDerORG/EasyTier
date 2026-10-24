---
id: ODS-T02
title: 字段迁移与网关组内 IPv4 唯一约束
status: todo
owner: null
requirements: [ODS-R01, ODS-R02, ODS-R03, ODS-R04, ODS-R06, ODS-R08]
plan: PLAN-ODS-01
depends_on: [ODS-T01]
integration_dependencies: []
---

# ODS-T02 字段迁移与网关组内 IPv4 唯一约束

- 所属计划：[PLAN-ODS-01 公共契约与持久化基础](../plan/plan-ods-01-contracts-and-storage.md)。
- 需求：[R01–R08 索引](../require/README.md)；具体验收按本任务范围与[公共契约](../plan/contracts/v1.md)关联。
- 状态：未认领、未开始；文档编写不代表实施。

## 目标与边界

提供发现资料、网关授权配置、同步快照/操作及唯一ADB服务的持久化结构，并把有组资产的IPv4唯一范围调整到当前网关组。输入T01契约与现有数据库约束；输出结构迁移、实体字段、兼容说明和待确认的历史服务状态。行为撤销触发器只归T15。

## 独占写入与步骤

拟新增 `easytier-web/src/migrator/m20261024_000019_openwrt_discovery.rs`；修改对应asset_devices/asset_services/asset_device_types及拟新增discovery实体。迁移注册和共用validation由T19处理。核对旧表内联唯一约束及全部外键/触发器，设计可回退的表迁移；同组IPv4唯一、跨组独立，无组资产保留既有范围；建立一device一ADB的数据库约束和endpoint_revision。新增资料默认未知、不伪造MAC；保存人工名/来源名分界及IPv6观察。旧服务保留，缺明确ADB类型标为待修正，不从旧automation_connections端口或默认5555造服务。

## 完成条件与交接

空库与升级路径、保留服务ID/公开关系、历史快照与新字段默认有清晰说明；不只改应用查询而遗漏数据库唯一约束。T03/T06/T07/T10/T11/T13/T15获得字段和事务约束，T19获得注册清单；没有声明结构迁移已验证。

## 后续验证

交T20验证迁移前后行数/FK、同组冲突/跨组同IP、ADB唯一、旧配置不生成服务、失败回退及重复迁移行为。本轮只文档，以上未执行。
