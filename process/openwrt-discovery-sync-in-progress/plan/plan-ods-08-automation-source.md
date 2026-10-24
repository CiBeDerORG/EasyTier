---
id: PLAN-ODS-08
title: 自动化主机数据源切换
status: ready
requirements: [ODS-R06]
tasks: [ODS-T13, ODS-T14]
---

# PLAN-ODS-08 自动化主机数据源切换

## 目标与依据

实现[ODS-R06](../require/require-ods-06-automation-service-source.md)，让主机列表、准备、投屏和脚本使用同一手机 ADB 服务。当前 `AutomationStore::list_devices/fingerprint/target` 与连接弹窗读取独立配置；核查基线及链接见需求文档。该计划不从旧配置生成服务，也不承担发现扫描。

## 小步骤与任务

1. ODS-T01 冻结手机/ADB 语义、目标版本、不可用原因和前后端契约；ODS-T02 建立结构化字段及单服务约束，明确历史多候选的待处理方式。
2. [ODS-T13](../task/task-ods-13-automation-service-source-api.md)替换列表、目标解析、fingerprint 和连接 API 的旧端口来源，统一授权与路由碰撞检查；移除 5555 兜底及旧启用条件。
3. [ODS-T14](../task/task-ods-14-automation-service-source-ui.md)接入服务来源列表，连接弹窗只展示权威端口；在 `AssetServiceDialog.vue` 增加结构化 ADB 类型/连接模式编辑及历史现存服务确认入口。保存复用 [T10](../task/task-ods-10-adb-reconcile.md) 的服务 CRUD/唯一性/事件能力，不从旧配置补建服务；保留配对授权操作，展示缺失/冲突。
4. ODS-T19 统一接线共享类型、客户端和入口；验证前后端同一服务目标，再交 PLAN-ODS-09 消费目标变更结果。

## 并行、依赖与唯一写入

| 任务 | 开工依赖 | 仅联调依赖 | 独占实现范围 |
| --- | --- | --- | --- |
| ODS-T13 | ODS-T02 | 无 | `db/automation/mod.rs`、`restful/automation/devices.rs` |
| ODS-T14 | ODS-T01 | ODS-T10、ODS-T13 | `AutomationCenter.vue`、`automation/ConnectionDialog.vue`、`node-assets/AssetServiceDialog.vue` |

T14 可在契约稳定后与 T10/T13 并行，不等待真实后端才能制作页面；它使用契约样例的结果不代表接口已联调。`services.rs` 的服务类型、模式、唯一约束与事件写入仅归 T10；公共 DTO、API 客户端、校验及路由由 T01 定义、T19 接线，各任务不得互改。

## 交付与后续验证

交付来源解析、主机消费、现存服务结构化确认/编辑及旧客户端处理。检查 ODS-R06-A01 至 ODS-R06-A05，覆盖历史服务缺类型、手工编辑、旧配置冲突、无服务、多候选及跨账号权限。确认前不任选端口，来源切换不隐式补建服务。应用验证须在后续获授权时执行；本轮仅文档，未运行测试、构建或 CI。
