---
feature: network-node-attributes
status: done
baseline: origin/main@e307dadb
updated_at: 2026-10-22
---

# 网络节点属性：需求

[需求/计划汇总](../README.md) · [计划](../plan/README.md) · [任务看板](../task/README.md)

## 已确认的范围

|编号|需求|实现依据|
|---|---|---|
|NAT-01|在仓库 main 上扩展已有网络节点，不引入用户管理分支的功能|原 `devices` 以 `(user_id, machine_id)` 标识登记节点；沿用现有归属和登录会话|
|NAT-02|手动维护地点、设备型号，允许清空|扩展现有表；两个手填字段独立更新，不被注册、心跳或 IP 观察覆盖|
|NAT-03|记录直连 config-server 的来源 IP 和 IP 推测所在地|采用成功认证连接的服务端 peer IP；TCP/UDP 经 NAT 时为服务器观察到的出口 IP|
|NAT-04|在当前 device 单项增加独立“编辑属性”入口|沿用 `DeviceList.vue` 的卡片、表格、手机列表及现有操作；地点和型号可在单卡查看，自动信息只读|
|NAT-05|部署本次实现并提供真实页面截图|使用当前云环境、独立演示数据库和端口；演示节点明确标注，截图来自实际运行后端|

手填字段仅地点、设备型号，没有备注。保留现有别名、删除、管理、配置、搜索、刷新和视图切换；不增加页面或重排主要操作。不改 config-server 客户端协议、OIDC/账号功能，不增加拓扑资源、导航、端口映射、定时任务、额外轮询或外部 GeoIP HTTP 调用。

## 数据约定

字段直接扩展 `devices`，不新建 machine 或拓扑实体。`site_location` 是用户填写的实际地点，与 IP 推测所在地分别保存。

|字段|含义|写入方及约束|
|---|---|---|
|`site_location`|手填地点|用户；可空；去除两端空白后最多 256 个 Unicode 字符|
|`device_model`|手填设备型号|用户；可空；去除两端空白后最多 128 个 Unicode 字符|
|`connection_ip`|本次有效连接的来源 IP|服务端；IPv4/IPv6 规范化；可空|
|`ip_country`|IP 推测国家|本地 GeoIP；可空|
|`ip_region`|IP 推测区域|本地 GeoIP；可空|
|`ip_city`|IP 推测城市|本地 GeoIP；可空|
|`ip_location_status`|定位结果|`unknown`、`resolved`、`non_public`、`not_found`、`unavailable`；旧记录默认 `unknown`|
|`ip_observed_at`|服务端观察时间|随 IP 快照保存；旧记录可空|

自动属性在首次成功认证或重连时形成完整快照，不在每次心跳重复写入。非公网 IP 不查询所在地；定位失败时保存本次 IP 和失败状态，清除旧所在地，避免把上次地点套在新 IP 上。元数据写入失败不能阻止客户端正常认证、配置和连接。离线节点仍可查看已存属性。

现有内置 GeoIP 为 Country 数据库，只能承诺其实际具备的精度；区域/城市允许为空。可复用现有本地数据库配置，不自动下载城市库。

## 前后端固定契约

`GET /api/v1/machines` 保留现有响应结构，在每项附加上述八个字段。外部 webhook 认证模式不开放本次手填更新；自动元数据按该模式现有兼容路径返回空值/`unknown`。

新增 `PUT /api/v1/machines/{machine-id}/attributes`：

```json
{"site_location":"家中机柜","device_model":"OpenWrt R4S"}
```

两个键均为必填字符串；空字符串或纯空白表示清空。去除两端空白之前拒绝控制字符；按 Unicode 字符计数。拒绝未知键、空值和错误类型，不能借该接口写入来源 IP、所在地、用户归属等字段。成功返回 HTTP 200 和标准化后的两字段 JSON，清空字段为 `null`。

从已登录会话取得 `user_id`，按 `(user_id, machine_id)` 一次性更新两个字段。节点不存在或不属于当前用户返回 404，不创建节点；外部 webhook 认证模式返回 403；参数错误返回 400。登录边界沿用现有 network API 中间件。数据库错误沿用现有错误处理，不允许部分保存。

## 验收项与当前问题

NAA-01 至 NAA-06 已完成本次范围的验证；命令、结果及实际部署边界见[验收记录](../verification.md)。

|编号|验收项|对应任务|
|---|---|---|
|NAA-01|旧数据库迁移后节点标识、别名及已有配置可读取；新字段默认值正确|NAT-001|
|NAA-02|本人可保存、修剪空白和清空；越权、未知节点、非法字段及超长输入不改数据；两个字段原子更新|NAT-001|
|NAA-03|原页面操作仍可用；弹窗取消不保存、提交防重复、错误保留草稿；已有列表刷新不覆盖正在编辑的草稿|NAT-002|
|NAA-04|真实后端保存后刷新页面仍可读取；设备心跳、重连及 IP 快照更新不覆盖手填属性|NAT-001、NAT-003|
|NAA-05|有效 TCP/UDP 连接观察 IP；非公网、无命中、定位不可用及写入失败按约定处理，旧连接不能覆盖新快照|NAT-003|
|NAA-06|本次构建部署可访问；提供卡片属性、编辑弹窗及窄屏截图，记录真实验证结果和 Country 库精度限制|NAT-003|

保存与重连已验证，截图来自真实后端；内置 Country 库仍无城市精度保证。实际用户 OpenWrt 的公网接入尚未现场验证，当前端到端证据为云环境内真实客户端的 TCP 直连。详见[根汇总](../README.md)。

## 代码依据

- [设备实体](../../../easytier-web/src/db/entity/devices.rs)、[设备持久化](../../../easytier-web/src/db/devices.rs)：沿用登记主键和心跳更新边界。
- [机器列表及属性 API](../../../easytier-web/src/restful/network.rs)：复用列表、会话归属和中央管理边界。
- [设备列表](../../../easytier-web/frontend/src/components/DeviceList.vue)、[前端 API](../../../easytier-web/frontend/src/modules/api.ts)：沿用原交互和刷新方式。
- [连接管理](../../../easytier-web/src/client_manager/mod.rs)、[Session](../../../easytier-web/src/client_manager/session.rs)、[Storage](../../../easytier-web/src/client_manager/storage.rs)：接入已讨论的 IP 观察，不移入用户管理分支特有代码。
