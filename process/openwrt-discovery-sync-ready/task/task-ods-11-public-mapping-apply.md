---
id: ODS-T11
feature: openwrt-discovery-sync
title: 公开映射目标更新与运行态应用
owner: null
status: todo
depends_on: [ODS-T02]
integration_dependencies: [ODS-T10]
requirements: [ODS-R05, ODS-R08]
plan: PLAN-ODS-07
---

# ODS-T11 公开映射目标更新与运行态应用

所属：[ODS-R05](../require/require-ods-05-public-mapping.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)；计划：[PLAN-ODS-07](../plan/plan-ods-07-publication-sync.md)。硬依赖T02，T10仅是ADB完整联调前提，当前未认领。

## 输入与输出

输入为服务/设备的新旧目标、当前版本及覆盖保留清单。输出事务内期望配置、公开关系迁移结果、提交后应用描述和独立重试结果。字段与辅助边界只引用[公共契约](../plan/contracts/v1.md)。

## 独占拟改范围

- 既有 `easytier-web/src/db/assets/publications.rs`：`sync_service`、`sync_device_address`、`remove_service`及拟新增内部辅助 `transfer_service_publication`，供T08在事务内迁移保留关系。
- 既有 `easytier-web/src/restful/assets/publication_apply.rs`：`apply`及版本化应用/重试适配。
- 拟新增 `easytier-web/src/db/discovery/publication_sync.rs`：拟新增内部 `prepare_publication_changes` 编排目标变化，导出声明由T01冻结。
- 只读服务CRUD、网络配置存储和运行RPC；不改T02迁移、T07设备、T10ADB、自动化或共享路由。

## 小步骤

1. 从已归属服务推导目标；ADB端口跟随服务更新，设备地址取派生组网IP。保留公网节点/实例、协议、绑定IP和绑定端口。
2. 提供T08可在同一事务调用的关系迁移辅助，保留服务ID并换deviceID；删除未保留项时只清理自有规则，保护配置代际和外部改动。
3. 事务内保存资产对应的期望转发配置；提交后再调用既有运行应用链，分别返回保存与应用状态，不能把通知已发出当成生效。
4. 持久化待应用版本并幂等重试。旧版本重试不得覆盖较新目标；失败保留已保存服务与原因，不重复创建设备/映射。

## 完成标准与未来验证

对应ODS-R05-A01～A06。未来验证公开端口保持、目标IP/端口更新、覆盖迁移、离线/拒绝、配置代际冲突、连续更新乱序重试和无关规则保留。真实网络应用需T21验收；本轮测试、构建、CI均未执行。

## 交接

向T08提供迁移辅助；向T10/T19提供数据库与提交后应用适配；向T17提供有效目标版本和阶段状态。T19负责共享入口接线，T11不反向调用发现或触发未授权的真实扫描。
