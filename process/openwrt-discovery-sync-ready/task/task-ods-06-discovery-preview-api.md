---
id: ODS-T06
title: 普通发现归一化与预览接口
owner: null
status: todo
depends_on: [ODS-T03, ODS-T04]
integration_dependencies: []
requirements: [ODS-R02, ODS-R08]
plan: PLAN-ODS-03
---

# ODS-T06 普通发现归一化与预览接口

所属[ODS-R02](../require/require-ods-02-device-discovery-review.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-03](../plan/plan-ods-03-discovery-preview.md)。开发依赖 T03/T04，无额外联调前置。

## 输入与输出

输入为合法网关上下文及一次普通发现请求。拟新增 `POST /api/v1/assets/discovery/gateways/{gateway_device_id}/devices/scan` 生成预览；输出来源快照、逐行差异、同组候选、允许动作/阻塞原因及版本。字段只采用[契约 v1](../plan/contracts/v1.md)，正式资产在此阶段不变。

## 独占写入与非写入范围

拟新增 `easytier-web/src/discovery/device_preview.rs`、`src/db/discovery/preview_store.rs`、`src/restful/discovery/preview.rs`（后两者同属 `easytier-web/`），分别负责归一化/匹配、预览保存、处理器。不得写正式资产应用、覆盖事务、迁移、LuCI 客户端、共享 DTO 或路由；前端清单归 T09。

## 可执行步骤

1. 调 T03/T04 扫描一次，区分快照时间与尝试时间；保存部分成功、旧快照、无快照和截断语义。
2. 规范化单 IPv4、MAC、hostname、来源及时间；IPv6只记录，多IPv4待处理，缺字段不形成清空动作。
3. 按当前组非网关自身子设备的 IPv4 初步匹配；无命中提供同组手选候选，显示 MAC/发现名称差异及可保留服务摘要，不能把人工名称差异反复当新冲突。
4. 将历史同 IP 新旧记录、重复目标和地址规则缺失列为阻塞/待处理；保存网关配置/绑定及目标设备/服务版本，供 T07/T08 重验。不允许组外目标或发起网关；版本变化要求刷新预览。

## 完成条件与交接

满足 ODS-R02-A01～A07。失败/空结果不输出清空资产指令；同组重复 IP 不能全选新建。向 T07/T08 交付预览和版本校验接口，向 T09 交付差异/动作模型，向 T10 交付可复用的单次普通发现与目标过滤能力；模块注册交 T19。

## 验证计划与代码依据

后续夹具覆盖历史 IP 复用、未知 MAC/名称、跨组同 IP、手选目标、旧预览及各来源状态。**本轮未执行：仅文档，不运行应用验证。**

依据：[LuCI format.uc](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/share/easy-discovery/discovery/format.uc) 的 `devices/add/findScope`；现有应用约束：[db/assets/devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/devices.rs) 的 `validate_relations`，组级唯一性由 T02 提供。
