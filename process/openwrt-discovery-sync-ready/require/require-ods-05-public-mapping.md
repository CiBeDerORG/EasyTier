---
id: ODS-R05
feature: openwrt-discovery-sync
title: 公开映射随目标更新
status: ready
owner: null
plans: [PLAN-ODS-07]
---

# ODS-R05 公开映射随目标更新

产品规则已确认，功能尚未实施。采用[契约 v1](../plan/contracts/v1.md)中的变更与阶段结果结构；数据库保存和运行网络应用分别说明结果。

## 已确认规则

1. 手机现有 ADB 服务的端口变化时，若服务已经公开，同步更新其映射目标端口。设备组网地址变化时，更新其保留服务的公开目标地址。目标地址采用网关规则派生的组网地址；不能误用在服务器端不可达的原始内网 IP。
2. 公网服务器、网络实例、传输协议、公网绑定地址与绑定端口保持原选择。同步改变的是目标地址/目标端口，不重新分配公网入口、不把未公开的服务自动公开。非 ADB 服务仅因设备地址变化而更新目标 IP，不擅自改其端口。
3. 覆盖时勾选保留的公开服务维持服务 ID 和公开关系，重新归属新设备并更新目标；未勾选服务清理其拥有的转发规则。不能删除同一公网配置里的其他人工规则、其他服务规则或已被外部修改而无法确认归属的规则。
4. 资产、服务、公开关系和预期网络配置在数据库事务内保持一致；事务失败整体回滚。事务提交后再调用已有运行配置应用机制，避免在数据库锁内等待远端网络。
5. 必须区分“数据库目标已保存”“运行映射已应用”“应用失败/待重试”。保存成功而公网节点离线、网络变更或调用失败时，保留已保存状态和失败原因，不能报全链路成功，也不能删除新 ADB 服务来掩盖应用失败。
6. 重试只应用当前仍有效的期望配置。服务再改端口、被删除或覆盖后，旧重试不得写回旧目标；重复请求不得追加重复转发规则。遇到外部修改、实例代际变化或规则所有权冲突，显示可处理冲突，不强行覆盖现网配置。

## 场景与验收

| 验收编号 | 场景 | 必须观察到的结果 |
| --- | --- | --- |
| ODS-R05-A01 | 已公开 ADB 从旧端口切到新端口 | 服务、保存配置和运行映射使用新目标端口，公网入口保持原值 |
| ODS-R05-A02 | 手机内网 IP 变化并派生出新组网 IP | 保留公开服务的目标 IP 跟随变化，其他设备和服务不受影响 |
| ODS-R05-A03 | 覆盖中部分公开服务保留、部分不保留 | 保留项 ID/公网关系迁到新设备；删除项仅清理自己拥有的映射 |
| ODS-R05-A04 | 数据库已提交但公网节点离线或运行应用失败 | 分别显示保存成功与应用失败；重试不重复保存资产或创建服务 |
| ODS-R05-A05 | 连续改两次端口，第一次的重试较晚到达 | 旧目标不得覆盖新目标；同版本重复应用不产生重复规则 |
| ODS-R05-A06 | 公网配置被手工修改或实例被删除重建 | 报告版本/归属冲突，不删除无关规则；保留可核对的阶段结果 |

## 代码证据与缺口

- [`db/assets/publications.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/publications.rs) 的 `sync_service`、`sync_device_address`、`remove_service` 已维护持久化关系与目标地址，目标取设备 `overlay_ip`；`remove_owned` 按原配置身份处理归属。选择性覆盖的关系迁移和发现结果版本交接属于拟新增设计。
- [`db/assets/services.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/services.rs) 的创建/更新/删除均在事务中同步公开关系，已有服务端口唯一检查可复用。
- [`restful/assets/mod.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/restful/assets/mod.rs) 的 `notify_service_publication` 调用 [`publication_apply.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/restful/assets/publication_apply.rs) 的 `apply`，并把 `publication_sync` 写入响应。发现同步需要接入这条运行应用链，不能只改数据库。

## 实施与验证

[PLAN-ODS-07](../plan/plan-ods-07-publication-sync.md) / [ODS-T11](../task/task-ods-11-public-mapping-apply.md) 实现数据库迁移辅助及运行应用；T08 使用它迁移覆盖保留项，T10/T19 接入 ADB 变更，T17协调后续连接状态。当前只读核对与文档拆分完成情况见根汇总；以上行为尚未实现或验证。
