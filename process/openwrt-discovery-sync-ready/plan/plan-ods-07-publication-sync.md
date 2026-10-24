---
id: PLAN-ODS-07
feature: openwrt-discovery-sync
title: 公开映射更新与应用
status: ready
owner: null
requirements: [ODS-R05, ODS-R08]
tasks: [ODS-T11]
---

# PLAN-ODS-07 公开映射更新与应用

## 目标与代码基线

实现 [ODS-R05](../require/require-ods-05-public-mapping.md) 与 [ODS-R08](../require/require-ods-08-results-and-acceptance.md)：已公开服务保持公网入口，随设备地址/ADB 端口更新目标，并把持久化结果与运行应用结果分开。`dbbade81` 已有 `publications::sync_service`、`sync_device_address` 和 `publication_apply::apply`；覆盖时保留关系迁移、发现操作版本关联和分阶段重试属于本次拟扩展。

## 小任务与并行顺序

[ODS-T11 公开映射目标更新与运行态应用](../task/task-ods-11-public-mapping-apply.md) 硬依赖 T02。先实现事务内目标更新/关系迁移辅助，再完善提交后的运行应用及重试结果。T10 是 ADB 全链路联调前提，不是实现辅助函数的硬依赖。

T11 可与 T07、T09、T10 并行，独占公开模块；T08 必须在 T11 的迁移辅助可用后再实现覆盖关系转移。T17 消费版本化目标变更，T19 唯一负责把普通同步、ADB 同步与运行应用接到共同入口。

## 文件所有权与公共契约

T11 唯一修改既有 `db/assets/publications.rs`、`restful/assets/publication_apply.rs`，并新增 `db/discovery/publication_sync.rs`。不改资产/服务 CRUD、发现匹配、自动化执行器或数据库迁移；其所需接口和状态存储由 T01/T02 预留。公开关系转移、待应用描述和阶段状态只定义于[公共契约](contracts/v1.md)，各调用方引用，不复制第二套规则。

## 交接与验收门槛

- 交给 T08：保持服务 ID/公网入口的关系转移辅助，以及未保留项的受控清理能力。
- 交给 T10/T19：事务内更新期望配置与提交后应用的边界；调用失败不撤销已保存资产。
- 交给 T17：当前有效目标版本、应用阶段状态；重试不能让旧目标覆盖新目标。
- 交给 T20/T21：ODS-R05-A01～A06 的数据库、故障及真实公网场景。

完成时必须能区分保存成功、运行应用成功和应用待重试；不得将“已通知节点”当作“已确认生效”。本轮仅拆分文档，任务未认领、代码未实施，测试/构建/CI 未执行。
