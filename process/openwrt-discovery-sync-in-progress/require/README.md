# 小需求索引

- 需求状态：已确认，可据此实施；源码实现尚未开始。
- 代码基线：`feat/automation-center@dbbade81`；LuCI：`main@d3dbe4f`。
- 公共实施契约：[v1](../plan/contracts/v1.md)。各需求规定业务与验收，接口/字段只在公共契约维护。

| 编号 | 小需求 | 主要实施计划 |
| --- | --- | --- |
| ODS-R01 | [网关发现接入与授权](require-ods-01-gateway-access.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R02 | [普通设备发现与逐条确认](require-ods-02-device-discovery-review.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-03](../plan/plan-ods-03-discovery-preview.md)、[PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R03 | [设备新增、更新与选择性覆盖](require-ods-03-asset-apply-overwrite.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-04](../plan/plan-ods-04-device-apply.md)、[PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R04 | [ADB 发现与唯一服务维护](require-ods-04-adb-discovery-service.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R05 | [公开映射随目标更新](require-ods-05-public-mapping.md) | [PLAN-ODS-07](../plan/plan-ods-07-publication-sync.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R06 | [自动化主机统一使用手机 ADB 服务](require-ods-06-automation-service-source.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-08](../plan/plan-ods-08-automation-source.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R07 | [即时重连、画面恢复与单机退出脚本](require-ods-07-reconnect-script-exit.md) | [PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |
| ODS-R08 | [分阶段结果、失败恢复与验收](require-ods-08-results-and-acceptance.md) | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md)、[PLAN-ODS-03](../plan/plan-ods-03-discovery-preview.md)、[PLAN-ODS-04](../plan/plan-ods-04-device-apply.md)、[PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md)、[PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md)、[PLAN-ODS-07](../plan/plan-ods-07-publication-sync.md)、[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)、[PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) |

## 已确认约束

对象为网关自身的 OpenWrt 设备；导入当前组；IPv4组内唯一、跨组独立；历史记录可逐条选择；覆盖逐项保留服务；ADB最多补一次普通发现，仍无匹配跳过；一手机一ADB服务；公开目标随端点变化；自动化只读现有手机ADB服务；重连后当前手机退出旧脚本，其他手机继续。

验收编号统一为 `ODS-Rxx-Ayy`。需求ready只表示规则确认，不表示任何应用测试通过。入口：[需求汇总](../README.md)、[计划](../plan/README.md)、[任务看板](../task/README.md)。

