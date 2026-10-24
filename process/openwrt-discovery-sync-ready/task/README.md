# OpenWrt发现同步任务看板

- 基线：`feat/automation-center@dbbade81`；文档拆分日期：2026-10-24（北京时间）。
- 实现进度：**0/22**；未认领22，进行中0，完成0。
- 负责人均为 `null`，文档作者不代表实施负责人。
- 公共契约：[v1](../plan/contracts/v1.md)；[计划与所有权](../plan/README.md)；[需求](../require/README.md)。

## 任务清单

| 任务 | 内容 | 计划 | 硬依赖 | 联调依赖 | 状态 | 负责人 |
| --- | --- | --- | --- | --- | --- | --- |
| [ODS-T01](task-ods-01-contract-and-shared-skeleton.md) | 公共契约冻结与共享入口骨架 | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md) | 无 | 无 | todo | null |
| [ODS-T02](task-ods-02-schema-and-scoped-identity.md) | 字段迁移与网关组内 IPv4 唯一约束 | [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md) | ODS-T01 | 无 | todo | null |
| [ODS-T03](task-ods-03-gateway-access-api.md) | 网关接入配置与授权接口 | [PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md) | ODS-T02 | ODS-T04 | todo | null |
| [ODS-T04](task-ods-04-luci-rpc-client.md) | LuCI RPC 认证与发现客户端 | [PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md) | ODS-T01 | ODS-T03 | todo | null |
| [ODS-T05](task-ods-05-gateway-access-ui.md) | 网关基本信息与发现操作入口 | [PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md) | ODS-T01 | ODS-T03 | todo | null |
| [ODS-T06](task-ods-06-discovery-preview-api.md) | 普通发现归一化与预览接口 | [PLAN-ODS-03](../plan/plan-ods-03-discovery-preview.md) | ODS-T03、ODS-T04 | 无 | todo | null |
| [ODS-T07](task-ods-07-device-create-update.md) | 按预览执行新增与更新 | [PLAN-ODS-04](../plan/plan-ods-04-device-apply.md) | ODS-T02、ODS-T06、ODS-T15 | ODS-T11、ODS-T17 | todo | null |
| [ODS-T08](task-ods-08-device-overwrite-services.md) | 覆盖重建与服务勾选保留 | [PLAN-ODS-04](../plan/plan-ods-04-device-apply.md) | ODS-T07、ODS-T11 | ODS-T17 | todo | null |
| [ODS-T09](task-ods-09-discovery-review-ui.md) | 同步差异清单与操作选择 | [PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md) | ODS-T01 | ODS-T06、ODS-T07、ODS-T08 | todo | null |
| [ODS-T10](task-ods-10-adb-reconcile.md) | ADB 匹配、补发现与单服务维护 | [PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md) | ODS-T02、ODS-T04、ODS-T06、ODS-T07 | ODS-T11、ODS-T17 | todo | null |
| [ODS-T11](task-ods-11-public-mapping-apply.md) | 公开映射目标更新与运行态应用 | [PLAN-ODS-07](../plan/plan-ods-07-publication-sync.md) | ODS-T02 | ODS-T10 | todo | null |
| [ODS-T12](task-ods-12-adb-discovery-ui.md) | ADB 发现结果与分阶段状态 | [PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md) | ODS-T01 | ODS-T10、ODS-T17 | todo | null |
| [ODS-T13](task-ods-13-automation-service-source-api.md) | 自动化目标改读手机 ADB 服务 | [PLAN-ODS-08](../plan/plan-ods-08-automation-source.md) | ODS-T02 | 无 | todo | null |
| [ODS-T14](task-ods-14-automation-service-source-ui.md) | 自动化主机列表与服务端口入口 | [PLAN-ODS-08](../plan/plan-ods-08-automation-source.md) | ODS-T01 | ODS-T10、ODS-T13 | todo | null |
| [ODS-T15](task-ods-15-script-device-exit-api.md) | 后端按设备退出当前脚本 | [PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md) | ODS-T02 | ODS-T13、ODS-T16 | todo | null |
| [ODS-T16](task-ods-16-executor-device-exit.md) | 执行器单设备退出与旧目标清理 | [PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md) | ODS-T01 | ODS-T15 | todo | null |
| [ODS-T17](task-ods-17-reconnect-coordinator.md) | 服务端口事件与重连协调 | [PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md) | ODS-T10、ODS-T11、ODS-T13、ODS-T15、ODS-T16 | 无 | todo | null |
| [ODS-T18](task-ods-18-reconnect-ui.md) | 重连状态与原投屏恢复 | [PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md) | ODS-T01 | ODS-T14、ODS-T17 | todo | null |
| [ODS-T19](task-ods-19-integration-wiring.md) | 共享入口接线与全链路复审 | [PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) | ODS-T05、ODS-T08、ODS-T09、ODS-T10、ODS-T11、ODS-T12、ODS-T13、ODS-T14、ODS-T15、ODS-T16、ODS-T17、ODS-T18 | 无 | todo | null |
| [ODS-T20](task-ods-20-automated-regression.md) | 迁移、接口与故障契约回归 | [PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) | ODS-T19 | 无 | todo | null |
| [ODS-T21](task-ods-21-real-device-acceptance.md) | 真实 OpenWrt、手机与公开映射验收 | [PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) | ODS-T20 | 无 | todo | null |
| [ODS-T22](task-ods-22-delivery-and-rollback.md) | 交付、升级与回退记录 | [PLAN-ODS-10](../plan/plan-ods-10-integration-and-delivery.md) | ODS-T21 | 无 | todo | null |

## 认领与交接

1. 实施时先核对任务硬依赖、最新源码和独占文件；将本任务owner/status同步至本文件及根汇总。
2. 按冻结契约写自己模块；公共入口交T19，不能为编译方便同时改其他任务的共享文件。
3. 任务完成说明实际实现、未覆盖边界、后续验证。缺环境/授权如实记录，不能记为通过。
4. 条件变化导致接口/文件范围调整时，先更新唯一契约、对应任务和依赖，再通知交接方。
5. 需求目录保持ready直到开始实施，再由协调者整体改为in-progress并更新入口链接；只写完文档不能标done。

## 后续验证门槛

T20、T21需要后续测试授权和具体范围，T22需要构建/发布/部署的对应授权。用户本次“拆分需求计划与任务计划”只授权文档工作。当前没有任何应用测试、构建、真实发现或部署执行结果。

