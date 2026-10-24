---
id: ODS-R03
feature: openwrt-discovery-sync
title: 设备新增、更新与选择性覆盖
status: ready
owner: null
plans: [PLAN-ODS-04, PLAN-ODS-05]
---

# ODS-R03 设备新增、更新与选择性覆盖

产品规则已确认，功能尚未实施。公共字段、操作枚举和结果结构只在[契约 v1](../plan/contracts/v1.md)维护；本文件规定行为，不另建接口定义。

## 目标与范围

把已审阅的普通发现记录写入发起发现的 OpenWrt 所属网关组。匹配范围固定为账号、节点、当前网关组；目标只允许该组的非网关自身子设备，不能覆盖发起发现的网关及其授权配置。历史发现可以登记，但登记不表示设备当前在线。IPv6 只记录，不参与身份定位或组网地址计算。

## 已确认规则

1. IPv4 是组内第一步定位条件，MAC 和发现名称用于显示差异；三者相同可作为无变化项处理，其他变化由用户逐条选择。MAC、发现名称与人工显示名称分别维护；缺失的发现字段不清空已有信息，普通发现不把手机降回普通类型。
2. 当前组已有该 IPv4 时禁用“新建”，可更新、覆盖或跳过；没有命中时可新建，也可手选同组旧设备更新或覆盖。手选目标不能跨组、不能是网关自身，也不能抢占该组另一设备仍持有的目标 IP。不同组可以登记相同 IPv4。
3. 更新保留原设备 ID，只应用已确认的基本资料、发现资料与地址变化，保留全部既有服务及其公开关系。跳过不写入；不能因本次结果未包含某设备就删除它。
4. 覆盖真正删除旧设备记录并生成新 ID。确认界面逐项展示服务，用户勾选保留项；保留服务 ID、服务内容及公开关系迁移到新设备，未保留的服务删除并清理其公开映射。若未保留 ADB 服务，新设备不再因此成为自动化主机；不从旧自动化端口补回服务。历史运行、会话及目标快照保留原身份。
5. 新建或 IPv4 变化必须按当前组的代理真实子网计算主机偏移和组网地址。唯一匹配规则才可应用；规则缺失、重叠歧义或绑定已变化时，该项保持待处理，不能猜地址或修改网络规则。公开目标更新遵循 [ODS-R05](require-ods-05-public-mapping.md)。
6. 每条记录作为一个原子应用单元，提交前重查权限、组归属、IPv4 占用、设备与服务版本。预览后发生变化时要求重新确认该项；失败不得留下半个新设备或丢失原服务。批量结果逐条报告，不把其他已成功项说成失败或重复应用。

## 场景与验收

| 验收编号 | 场景 | 必须观察到的结果 |
| --- | --- | --- |
| ODS-R03-A01 | 同组已有 IP，另一个组也使用相同 IP | 同组新建被阻止；跨组独立保存，任何操作均不修改另一组 |
| ODS-R03-A02 | 已有手机改了发现名称，返回的 MAC 或型号缺失 | 更新保留原 ID、手机类型、人工名称、缺失字段原值及所有服务 |
| ODS-R03-A03 | 新 IP 没命中，手选同组旧记录 | 更新保持 ID；覆盖生成新 ID；旧 IP 释放；跨组或网关自身目标被拒绝 |
| ODS-R03-A04 | IP 分别匹配一条、零条、多条代理规则 | 一条时地址正确派生；零条或多条时保持待处理并解释原因 |
| ODS-R03-A05 | 覆盖勾选部分服务，包含一个已公开服务 | 勾选服务 ID 不变并挂到新设备；未勾选项删除；公开关系按 R05 处理 |
| ODS-R03-A06 | 覆盖时保留或不保留 ADB，旧设备已有运行历史 | 仅保留 ADB 时新设备继续满足自动化数据源条件；旧历史和快照不删、不改写 |
| ODS-R03-A07 | 确认前服务变化、IP 被占用或事务写入失败 | 返回该项冲突或失败；设备、服务、绑定与公开关系完整回滚 |
| ODS-R03-A08 | 重复提交同一已确认操作，或选择跳过 | 不重复创建设备/服务；跳过及未返回设备不受影响 |

## 代码证据与缺口

以下均为 EasyTier `dbbade81d8edfd51bc3844c393fb9afc650b51d1` 已读事实：

- [`db/assets/devices.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/devices.rs) 的 `validate_relations` 按整个节点检查 LAN IP；`update_device` 固定归属；`delete_device` 先移除公开关系再依赖外键删除所有服务，不能直接用来实现选择性覆盖。
- [`address_derivation.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/address_derivation.rs) 的 `derive_address` / `rule_addresses` 已支持网关规则派生地址；[`binding_ops.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/binding_ops.rs) 已有绑定版本检查。
- [`asset_devices.rs`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/entity/asset_devices.rs) 尚无 MAC/发现资料字段；[`自动化迁移`](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/migrator/m20261024_000018_automation_center.rs) 的资产删除触发器会连带取消整次运行。T15唯一负责行为迁移，T07必须等其交付；不能沿用后宣称其他手机不受影响。

## 实施入口

- [PLAN-ODS-04：应用事务](../plan/plan-ods-04-device-apply.md)，负责 T07、T08。
- [PLAN-ODS-05：确认交互](../plan/plan-ods-05-device-sync-ui.md)，负责 T09。
- 当前验证：未执行；本轮仅需求与文档拆分。
