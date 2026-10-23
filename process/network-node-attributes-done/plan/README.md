---
feature: network-node-attributes
status: done
updated_at: 2026-10-22
---

# 网络节点属性：计划汇总

[需求/计划汇总](../README.md) · [需求与接口契约](../require/README.md) · [任务看板](../task/README.md)

基线为 `origin/main@e307dadb`，实施分支为 `feat/node-attributes-main`。先固定字段和接口，再按文件所有权并行实现，最后统一联调、部署和截图。三项计划已完成，验证证据见[验收记录](../verification.md)。

|计划|内容|状态|负责人|依赖与验收门禁|
|---|---|---|---|---|
|[PLAN-NAT-001](plan-nat-001-storage-api.md)|八个属性的迁移、存储及手填 API|已完成|后端 agent|按已固定契约独立实现；NAA-01、NAA-02 已通过|
|[PLAN-NAT-002](plan-nat-002-card-editor.md)|原设备列表的独立编辑入口和弹窗|已完成|前端 agent|与后端并行；依契约实现，不等待真实 API 才开工|
|[PLAN-NAT-003](plan-nat-003-integration-deploy.md)|IP 后端移植、联调、部署和截图|已完成|root|IP 源文件可并行移植；真实联调和部署必须等待前两项交付|

## 并行边界

|唯一写入负责人|文件归属|交接对象|
|---|---|---|
|后端 agent|`src/db/devices.rs`、`src/db/entity/devices.rs`、`src/db/mod.rs`、`src/migrator/mod.rs`、新增属性迁移、`src/restful/network.rs`|向前端交付接口契约，向 root 交付迁移及持久化实现|
|前端 agent|`frontend/src/components/DeviceList.vue`、`DeviceAttributesDialog.vue`、`DeviceDetails.vue`、`frontend/src/modules/api.ts`、`frontend-lib/src/modules/utils.ts`、中英语言包及对应前端测试|向 root 交付原列表中的编辑交互及验证入口|
|root|`src/client_manager/` 下本次 IP 来源、Session、Storage 和相关测试；部署文件及运行证据|整合三块实现，统一执行构建与部署验证|
|文档 agent|本目录的 require/plan/task 文档|完成后交由 root 更新实际进度与问题|

以上源码路径均相对 `easytier-web/`。具体文件列表以[任务明细](../task/README.md)为准；跨负责人修改须先交接，不同时改同一文件。共享类型 `utils.ts` 只由前端维护，迁移清单只由后端维护，IP 连接处理只由 root 维护。接口变更必须先同步需求契约，再通知其他负责人。

## 联调门禁

1. 两端实现达到固定契约，源码可编译，归属/清空/空值等差异已消除；移植未引入用户管理特有身份结构或迁移。
2. root 汇总数据库/API、连接快照、前端交互验证结果，再对实际运行后端进行保存、重载、取消和清空验证。
3. 使用独立演示数据部署；运行地址、构建来源和截图证据齐备后，才能把任务标为完成。

源码变更需要针对性验证。本次文档整理本身只做链接和内容核对；以后仅修改需求、计划或任务文档不触发测试、构建或 CI。
