---
id: ODS-T10
title: ADB 匹配、补发现与单服务维护
owner: Codex
status: done
depends_on: [ODS-T02, ODS-T04, ODS-T06, ODS-T07]
integration_dependencies: [ODS-T11, ODS-T17]
requirements: [ODS-R04, ODS-R08]
plan: PLAN-ODS-06
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T10 ADB 匹配、补发现与单服务维护

所属[ODS-R04](../require/require-ods-04-adb-discovery-service.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md)。T11/T17 只阻塞真实映射/重连联调，可先用冻结事件和适配器独立开发。

## 输入与输出

拟新增 `POST /api/v1/assets/discovery/gateways/{gateway_device_id}/adb/scan`，输入合法网关上下文和请求标识，输出 operation 状态、逐端点结果及必要的待处理 preview。实现同前缀 `POST /operations/{operation_id}/resolve` 的业务处理器；接收 T19 转交的 `devices/apply` 逐行结果，继续原操作。字段、事件与阶段统一采用[契约 v1](../plan/contracts/v1.md)。旧自动化端口不是输入。

## 独占写入与非写入范围

拟新增 `easytier-web/src/discovery/adb_reconcile.rs`、`src/db/discovery/adb_service.rs`、`src/restful/discovery/adb.rs`（后两者同属 `easytier-web/`）：分别负责流程、唯一服务事务和处理器。另独占现有 `easytier-web/src/db/assets/services.rs` 的 ADB 类型、唯一服务写入及手工端口事件。

不改迁移、普通资产事务、公开映射实现、自动化目标、执行器、共享类型和路由。现有共用 `validation.rs`、REST `assets/mod.rs` 由 T19 写；本任务交付字段校验 helper、服务方法和待接线清单，不抢写共享文件。

## 可执行步骤

1. 调 T04 获取 ADB 连接端点，限定当前组单 IPv4；配对服务不作连接端口。先检查本次有效性：`error/busy/has_snapshot=false` 和失败旧数组仅展示，不改类型/端口/公开关系，不生成重连事件；`partial` 只处理可证明本次有效的端点。
2. 存在未匹配 IP 时，整次最多调用一次 T06 普通发现；只处理这些 IP 的普通记录。保存 ADB 快照、`pending_ips`、补采已使用标志及到期时间，需选择的 preview 绑定 `originating_operation_id`；不导入其他设备。
3. 无歧义记录复用 T07；人工 `devices/apply` 后由 T19 转交逐行结果，T10 唯一负责沿原快照续接，含 skip 的终结处理，不重新扫描/补采。仍未匹配则跳过；原快照过期停止续接并要求显式新发现。
4. 多端口使用 `resolve`：核对 `expected_operation_revision`，只接受该行原快照的 `selected_endpoint_id`，拒绝任意端口和过期来源；保留请求幂等结果。
5. 事务内将匹配资产设为手机，并新建/更新唯一结构化 ADB 服务，保留服务 ID、公开关系和其他服务。旧服务缺类型待确认，不猜名称或旧自动化端口；其他服务占端口时报告冲突。
6. 现有手工改端口复用相同类型/唯一性 helper 和提交后事件；无变化不重连。交 T11/T17 分阶段处理，失败保留已保存事实；`retry` 的公开应用/连接/清理由 T19 分发给对应任务，不在此重跑扫描。

## 完成条件与交接

满足 ODS-R04-A01～A10；失败旧快照零写入，原操作续接不会第二次扫描/补采，未匹配不凭 ADB 地址建普通资产。向 T12 提供结果/受控端点选择，向 T11/T17 提供目标事件，向 T19 交付 scan/resolve 处理器、T07 应用结果续接入口及重试分发接线清单。

## 验证计划与代码依据

后续覆盖失败附带旧端口零写入、部分来源有效性、人工选择后的续接/过期、整次一次补采、端点越界选择、唯一服务与阶段幂等。**本轮未执行：仅文档，不运行测试或真实发现。**

依据：[LuCI adb 扫描入口](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/libexec/luci-easy-discovery-adb)、[format.uc 的 adb()](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/share/easy-discovery/discovery/format.uc)；结构化服务字段为 T02 拟新增。

## 本轮实际交付

ADB 仅当前有效端点；唯一服务、多端点选择、一次补发现及原快照续接已验证。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
