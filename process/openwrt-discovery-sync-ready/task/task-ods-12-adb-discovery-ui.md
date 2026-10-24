---
id: ODS-T12
title: ADB 发现结果与分阶段状态
owner: null
status: todo
depends_on: [ODS-T01]
integration_dependencies: [ODS-T10, ODS-T17]
requirements: [ODS-R04, ODS-R08]
plan: PLAN-ODS-06
---

# ODS-T12 ADB 发现结果与分阶段状态

所属[ODS-R04](../require/require-ods-04-adb-discovery-service.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md)。T10/T17 仅阻塞真实状态联调，独立组件先按冻结结果模型开发。

## 输入与输出

输入网关上下文、operation、逐端点结果及映射/连接阶段状态；输出关闭、读取进度、打开关联 preview、原快照端点选择及指定阶段重试事件。字段与事件只引用[契约 v1](../plan/contracts/v1.md)：同一网关前缀下 `GET /operations/{operation_id}` 只读，`POST /operations/{operation_id}/resolve` 仅提交原端点 ID，`POST /operations/{operation_id}/retry` 仅请求契约允许的失败阶段。

## 独占写入与非写入范围

唯一拟新增文件：`easytier-web/frontend/src/components/node-assets/AdbDiscoveryResultDialog.vue`。只负责 ADB 结果展示及交互事件，不写网关配置、行按钮、普通差异清单、投屏恢复组件、ApiClient、页面 composable、共享类型或路由；所有页面接线归 T19。

## 可执行步骤

1. 展示原操作的网关、扫描时间、端点与匹配设备，标出补采/待确认/跳过；失败附带缓存只显示历史信息，不能提供“应用旧端口”的操作。
2. 分别显示服务保存、公开映射、连接/未授权状态；`partial` 中不能证明本次有效的端点不提供写入选择，保存成功而连接失败不显示全部成功。
3. 关联 preview 交 T09 逐条选择，应用结果由 T19 交 T10 继续 `originating_operation_id`；本组件不重新扫描、补采或实现普通覆盖。快照过期提示显式重新发现。
4. 多端口仅从原快照列表选择 `selected_endpoint_id` 并发出 resolve，携带请求与操作版本；不能让用户临时填新端口。retry 仅提交 `publication/connection/cleanup` 及原事件 ID，不重放成功阶段或旧脚本。
5. 活动操作只读状态，终态停止；关闭/切换网关丢弃旧响应，不把后台已提交操作报告为撤销。扫描失败或快照过期需返回显式发现入口，不能把 retry 改成 scan。

## 完成条件与交接

满足 ODS-R04-A02/A04/A06～A10 及 ODS-R08 的阶段反馈要求。用户能区分失败旧快照、待确认、待重新发现及各写入/连接阶段；页面不暴露凭据。向 T19 交付 props/emits、只读/resolve/retry 适配点，向 T09 转交关联 preview，向 T18 交付连接状态展示约定。

## 验证计划与代码依据

后续覆盖失败旧端点不可应用、preview 应用续接不扫描、原快照端点选择、快照过期、受控阶段重试和切换上下文。**本轮未执行：只编写任务，不运行 UI 测试、类型检查或构建。**

源结果语义依据：[LuCI docs/API.md](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/docs/API.md)；挂载位置依据：[AssetDeviceTable.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/node-assets/AssetDeviceTable.vue)，该共享文件只由 T19 修改。
