---
id: ODS-R06
title: 自动化主机统一使用手机 ADB 服务
status: ready
baseline: dbbade81d8edfd51bc3844c393fb9afc650b51d1
---

# ODS-R06 自动化主机统一使用手机 ADB 服务

本需求已确认，实施尚未开始。总范围见[需求汇总](../README.md)，字段与版本定义以[公共契约](../plan/contracts/v1.md)为唯一来源；本文件不另定义第二套接口。

## 目标与规则

- **ODS-R06-01：主机来源。** 自动化中心直接列出当前账号权限范围内、具有手机类型语义且拥有唯一有效结构化 ADB 服务的资产设备。一台手机对应一个主机条目和一个 ADB 端口，不把普通电脑、网关或非 ADB 服务纳入主机来源。
- **ODS-R06-02：端口来源。** 列表、连接、配对、投屏、脚本准备和脚本执行均解析同一条 ADB 服务。实际连接地址沿用设备当前有效网络绑定推导的组网地址，端口取该服务；不得因保存了公网发布配置就改用公网入口。
- **ODS-R06-03：替换旧来源。** 旧 `automation_connections` 的端口、默认 5555 和启用标志不能作为服务不存在时的兜底，也不能反向补建 ADB 服务。有 ADB 服务与旧配置不一致时，以有效服务为准。无服务的手机不生成可执行主机条目。
- **ODS-R06-04：不猜历史数据。** 普通 TCP 服务、名称看似 ADB 的记录和多个候选服务，未经[结构化分类与唯一性规则](../plan/plan-ods-01-contracts-and-storage.md)解决，不得任选一个端口、按创建时间抢选或静默删除候选。历史缺类型的现存服务保留，并提供明确确认/编辑入口；用户确认其 ADB 类型、连接模式及唯一关系后才成为有效主机来源。
- **ODS-R06-05：一致性。** 目标版本包含服务身份、有效端口及真实路由绑定；名称、备注等展示修改不应触发端口重连。服务删除、替换、类型改变或地址失效后，旧目标不得继续接收新动作。授权范围仍按账号、节点和资产关系校验。
- **ODS-R06-06：页面入口。** 自动化连接配置展示权威服务端口，跳转设备服务表单编辑结构化 ADB 类型、连接模式及端口，不保留独立的第二个端口编辑入口。手工服务变更与发现同步走同一事件链路。配对/授权仍是独立操作，发现或确认服务不等于设备已授权或已连接。

## 典型场景

1. 现有手机 A 的 ADB 服务为 37123，旧连接配置为 5555：自动化列表及所有执行入口使用 37123。
2. 手机 B 只有旧连接配置、没有结构化 ADB 服务：不从旧值生成服务，不默认连接 5555；可从设备服务管理补齐资料。
3. 手机 C 有未分类或多条历史 ADB 候选：保留现存记录，允许进入服务表单确认类型/连接模式；唯一性未解决前不把任意一条送往执行器。
4. 手机 D 的服务公开到另一台服务器：主机仍引用 D 的 ADB 服务及当前组网目标，公开映射按[ODS-R05](require-ods-05-public-mapping.md)独立维护。

## 验收标准

| 编号 | 可观察结果 |
| --- | --- |
| ODS-R06-A01 | 仅手机且唯一有效 ADB 服务进入主机列表，同一手机不重复。 |
| ODS-R06-A02 | 旧端口与服务端口冲突时，列表、准备、投屏和脚本使用同一服务端口。 |
| ODS-R06-A03 | 无服务、非 ADB 或多候选均不触发默认连接、不反向新增服务。 |
| ODS-R06-A04 | 修改名称不打断连接；真实目标变化使旧目标失效，并交给 ODS-R07 处理。 |
| ODS-R06-A05 | 页面可确认/编辑现存服务的 ADB 类型、连接模式及唯一端口；账号切换、无权限和路由不可用时不使用旧目标。 |

## 已核实的代码依据

基线 `dbbade81` 中，[AutomationStore](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/automation/mod.rs) 的 `list_devices` 列出全部资产；`fingerprint`、`target` 和 `ensure_route_unambiguous` 读取独立连接配置并含 5555 默认值。这是需要替换的现状，不是上述需求已实现。

[devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/restful/automation/devices.rs) 的 `connection` 接受独立 `adb_port`；[ConnectionDialog.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/automation/ConnectionDialog.vue) 提供该输入；[AutomationCenter.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/AutomationCenter.vue) 渲染返回的全部设备。现有资产服务尚无结构化 ADB 分类，基础字段由 ODS-T02 增加。

[AssetServiceDialog.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/node-assets/AssetServiceDialog.vue) 当前只有通用协议和端口等字段，尚无 ADB 类型/连接模式确认入口。T14 补前端入口，T10 提供服务 CRUD、唯一性及目标事件，T19 统一协议与校验接线。

## 实施关联

[PLAN-ODS-08](../plan/plan-ods-08-automation-source.md)负责来源切换；[ODS-T13](../task/task-ods-13-automation-service-source-api.md)负责后端，[ODS-T14](../task/task-ods-14-automation-service-source-ui.md)负责前端。端口变化后行为见[ODS-R07](require-ods-07-reconnect-script-exit.md)。本轮仅形成文档，以上验收均未执行。
