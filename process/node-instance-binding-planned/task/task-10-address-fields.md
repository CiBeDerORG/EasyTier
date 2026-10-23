---
id: T10
status: verified
owner: frontend-address-fields
depends_on: [T08]
updated_at: 2026-10-23
---

# T10 规则、主机部分与地址预览子组件

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。归属[P04](../plan/plan-04-frontend-contract-and-editors.md)，唯一写入负责人为地址组件agent；与T09/T11并行，不修改父表单。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 输入与文件所有权

- 输入：T08规则/预览契约、[后端地址需求](../require/require-02-backend-binding-and-addresses.md)、T09约定的组件接口。
- 独占新增：`easytier-web/frontend/src/components/node-assets/AssetAddressFields.vue`、`easytier-web/frontend/src/composables/useInstanceAddressPreview.ts`。
- 不修改`AssetDeviceDialog.vue`、网关编辑器、公共DTO/API、页面控制器或locale；接线和key合并交T09/T12。

## 工作内容与输出

1. props固定为`scope, subject?, gateway?, addressSpec, disabled`，接收契约所需绑定摘要、规则集合、当前设备/旧地址和编辑模式；规则由绑定实例提供，组件不能自行选择另一实例。
2. 展示规则与两侧网段，输入主机部分并触发服务端预览。支持多规则显式选择；`/24`可用最后一段，其他前缀按冻结契约输入，不假设所有网段均为`/24`。
3. 展示返回的overlay/LAN地址、地址来源、warning、字段错误和配置变化反馈；普通代理两侧相同，映射按服务端结果展示。直属模式overlay从实例读取，LAN选填纯IPv4，新绑定不能借旧校验接收IPv6。预览仅辅助，保存仍由后端重算并核对配置。
4. 旧资料/实例异常保持现有值可读。旧下挂/自身设备通过本组件地址预览后由父表单address提交转换，组件不独立binding。网关更新规则后孩子不自动改址；缺失规则可在同实例范围重选。父网关已确认同ID新世代时，rule_key相同的孩子仍待逐条采纳新来源。`gateway_self`模式overlay取网关实例IP，网关IP变化时旧overlay单独核对；LAN选填，rule+host仅派生真实LAN。
5. 预览请求按操作触发，维护上下文、请求序号和输入签名；快速切规则、切账号/节点、关闭父弹窗后，旧结果无效。配置读取失败不显示预览成功，不静默清空已有地址。
6. 通过`update:addressSpec`和`preview-change`事件提供提交所需的规则/主机输入、预览状态、错误和选择变化，不直接保存资产、不改组、不发实例配置修改请求，不新增轮询。

## 交接与验收场景

输出组件接口说明、字段错误映射、文案key、配置指纹处理方式和父表单需清理的依赖。T09接入父表单，T12统一实接后端。

后续验收覆盖普通代理、映射、多规则、非`/24`、边界主机地址、内网IP唯一冲突、规则改变/删除、IPv4尚无值、旧IPv6资料显示、快速连续选择与返回乱序。预览和保存不得使用两套计算规则。当前仅编写任务，所有测试/构建/CI均未执行。

## 2026-10-23 实施交接

AssetAddressFields与useInstanceAddressPreview已接入：选择规则/主机位、完整IPv4预览、旧新值及配置来源对比；旧响应隔离。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
