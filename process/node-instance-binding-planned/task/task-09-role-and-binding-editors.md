---
id: T09
status: verified
owner: frontend-binding-editors
depends_on: [T08]
updated_at: 2026-10-23
---

# T09 角色优先表单与实例绑定编辑器

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。归属[P04](../plan/plan-04-frontend-contract-and-editors.md)，唯一写入负责人为绑定表单agent，与T10地址组件、T11页面任务并行。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 输入与文件所有权

- 输入：T08客户端、[前端流程](../require/require-03-frontend-workflows.md)、[工程契约](../plan/contracts/v2.md)。与T10先固定地址组件props/events，与T11固定网关/设备两类保存事件。
- 独占：`easytier-web/frontend/src/components/node-assets/AssetDeviceDialog.vue`、`GatewayGroupDialog.vue`，以及新增`InstanceBindingSelector.vue`、`GatewayBindingEditor.vue`（同目录）。
- 不写T10地址子组件、T11页面控制器、公共类型或locale。新文案key作为交接清单交T12合并。

## 工作内容

1. 统一新增先选网关/普通角色。普通默认直属，可选已有网关；直属选无子网转发实例，组内沿用网关实例。网关选择代理或映射实例，两类均含“不改址”普通代理。
2. 创建复用的实例选择器：props为`scope, role, subject?, modelValue`，输出`update:modelValue, invalidated`；按角色请求当前节点持久配置候选，显示同名辨识、地址来源、占用、不可选原因和手动重试。离线有配置仍可选；运行独有未保存、地址不确定和配置读取失败分别处理。
3. 创建网关编辑器并由两个新增入口共同使用；网关IP/规则来自实例，不再让新网关绕过绑定手填网段。网关管理列表保留空组、编辑和非空禁删反馈。
4. 对接T10地址组件。表单只提交契约输入并使用后端生成结果，不私自拼IP；变更角色/组/实例清掉依赖输入和旧预览。
5. legacy_unbound网关/直属记录提供binding补绑；已绑定提供同节点同实例显式核对，同ID重建需确认新记录身份。旧下挂/自身设备经instance-address预览与address PUT转换为继承来源，不独立binding。父网关采纳新世代后，孩子即使rule_key相同仍需逐条address采纳；网关IP变化时自身overlay单独核对。各apply带request_id。元数据、绑定、地址分别保存反馈，归属/角色固定，配置异常不禁普通保存。
6. 保留居中弹窗、取消即放弃、手动自身设备标识和既有幂等重试；普通/自身标识调整若保留，须经address操作同步模式/地址和唯一校验。保存不确定时锁定原payload；上下文变化、关闭或新选择使旧请求响应失效。

旧未绑定记录保留原手工IP/网段维护分支与校验，包含存量IPv6；新建或已补绑记录使用新模式，不让兼容分支绕过实例必选或修改派生地址。

## 输出与交接

- 向T11提供`{resource_kind:'gateway'|'device', record}`保存事件、打开新增/编辑/补绑/核对所需参数；补绑/核对/地址修正事件带同类resource_kind和id，网关成功刷新组目录，普通成功走设备定位展开。
- 向T10提供`scope, subject?, gateway?, addressSpec, disabled`；监听`update:addressSpec`和`preview-change`，资产最终保存由本任务父表单负责，子组件不保存或改组。
- 向T12交付组件依赖、文案key、状态覆盖及未接线项。T12只有交接完成后才可顺序修改本任务文件。

## 验收场景（待授权执行）

两个新增网关入口均无法绕过实例；角色/组/实例切换不会沿用旧地址；同名实例可辨；离线/无候选/未保存/占用有明确结果；旧记录补绑保留服务；同实例核对不迁移资产；网关自身设备仍最多一条；超时重试不重复创建。本轮不运行测试、构建、类型检查或CI。

## 2026-10-23 实施交接

两个网关创建入口复用GatewayBindingEditor；设备弹窗先角色后实例，区分元数据、绑定和地址提交，保留旧资料维护。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
