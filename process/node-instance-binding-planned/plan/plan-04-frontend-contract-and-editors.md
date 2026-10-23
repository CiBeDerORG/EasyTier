---
id: P04
status: completed
depends_on: [P01]
tasks: [T08, T09, T10]
updated_at: 2026-10-23
---

# P04 前端契约、实例选择与设备编辑器

**最新状态：** 本计划已完成对应开发、授权验证及当前云环境交付；实际结果和未覆盖项见[验证报告](../task/verification-report.md)与[交付记录](../task/release-record.md)。以下验收设计保留，不将未实测范围扩大为已通过。

目标是把已确认的角色优先流程接入现有弹窗，完成“选实例 → 读规则 → 地址预览 → 保存”的操作链。范围依据[前端需求](../require/require-03-frontend-workflows.md)和[v2契约](contracts/v2.md)，源码已接入并完成授权验证，范围见[验证报告](../task/verification-report.md)。

## 工作包与顺序

|任务|独立产出|依赖|
|---|---|---|
|[T08](../task/task-08-frontend-client.md)|类型、资产API客户端及错误/幂等兼容|T01冻结契约|
|[T09](../task/task-09-role-and-binding-editors.md)|统一新增角色、实例选择、共用网关编辑器、普通设备父表单|T08；与T10先约定组件接口|
|[T10](../task/task-10-address-fields.md)|独立的规则/主机部分输入组件、地址预览composable|T08；与T09并行，不修改父表单|

T09同一负责人维护`GatewayGroupDialog.vue`和`AssetDeviceDialog.vue`，统一两个网关新增入口；不得拆给两个agent各写一套绑定流程。T10独占新地址子组件；父表单接线由T09或最终T12在交接后完成。

## 对代码的增量

- 扩展`frontend/src/types/node-assets.ts`与`modules/api.ts`，继续cookie认证、401处理和带账号/节点的资产范围；不通过旧远端客户端拼管理员候选查询。
- 共用实例选择器根据角色加载服务端候选，展示名称/短ID、配置/地址来源、占用和不可选原因。候选、规则和预览响应均隔离账号、节点、角色和表单代次。
- 网关编辑器读取实例IP/规则，旧网段手填仅按兼容契约用于legacy资料；支持一次性补绑与同实例显式核对。普通设备表单分别承接直属实例和网关继承规则。
- 地址子组件以服务端预览为准，支持规则选择、主机部分、双IP结果、字段错误和warning；无定时请求，不单独保存资产或修改网络配置。

## 接口交接

T08提供类型及方法名后，T09/T10仅消费，不各自新增API请求。地址组件props固定为`scope, subject?, gateway?, addressSpec, disabled`，输出`update:addressSpec`与`preview-change`；T09父表单发起资产保存，子组件不保存或改组。所有字段定义与空值语义在T01/T08交接时固定。

网关保存返回网关结果，普通保存返回设备结果，父页面据类型刷新正确目录。legacy补绑、配置核对与一般元数据保存的能力要分开；普通资料编辑、服务和删除不因配置读取失败被统一禁用。

接口分流固定为：网关/直属设备使用binding preview/apply，旧网关下挂/自身设备继承来源的转换使用instance-address preview及address PUT。新直属LAN仅接受IPv4；父网关采纳同ID新世代后，孩子仍逐条确认来源，不按相同rule_key自动视为正常。

## 完成条件与边界

两个新增入口都遵守实例必选，切换选择不保留旧预览，超时重试沿用同一请求，实例异常可读可解释。组件提交代码后交T12与后端实接；需要的授权验收场景交T13，本计划不自动执行测试/构建/CI。既有身份认证、实例配置编辑、公网发布和拓扑画布不在本计划内。
