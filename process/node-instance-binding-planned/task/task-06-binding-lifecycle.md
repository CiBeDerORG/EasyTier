---
id: T06
plan: P03
status: verified
depends_on: [T01]
integration_dependencies: [T02, T03, T04]
owner_role: binding-lifecycle-agent
updated_at: 2026-10-23
---

# T06 绑定状态、差异预览与核对规则

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。实现[生命周期需求](../require/require-04-lifecycle-and-compatibility.md)的按需解析，遵守[v2契约](../plan/contracts/v2.md)。本任务只读取/计算状态与确认差异，实际写入由T05负责。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 唯一写入范围

- 独占新文件 `easytier-web/src/db/assets/bindings.rs`；模块注册交T12。
- 输入现有组/设备资料、新内联绑定身份/快照、T03当前实例读结果和T04规则/地址推导。
- 不修改 `groups.rs/devices.rs/common.rs/validation.rs`、HTTP、迁移或旧 `restful/network.rs`、central/Core/config-server写路径。

## 状态输出

|状态|判断含义|业务动作基线|
|---|---|---|
|legacy_unbound|旧资料尚无完整绑定|保留原资料，可明确首次补绑|
|current|配置行身份及相关指纹/规则仍匹配|正常管理，可按需核对|
|changed|同一配置行的相关配置或子规则来源变化|保留原快照/地址，展示差异并显式核对|
|missing|绑定scope/instance当前无持久配置行|保留资料，不拦原实例操作，不自动改绑|
|replaced|同scope同instance_id出现不同配置行ID/create_time|展示删除重建差异，明确确认后方可接纳|
|unavailable|读取失败、解析异常或当前无法确定必要配置|保留资料和原因，不能当作空规则或自动转直属|

仅IPv6、IPv4缺失、网关原规则不再存在等以契约中的reason细分。状态优先级和状态/动作组合以v2统一，不能两个组件各自推断。节点离线本身不是missing；服务与设备网络可达性不从这些状态推导。

## 差异预览和确认边界

1. GET/list仅计算状态，不更新行身份、快照、IP或确认时间；不启动定时任务或后台重试。
2. 预览比较旧登记身份/指纹与当前同scope同instance_id，输出精简的原/新实例IP、规则、来源及孩子影响。完整secret配置不进入差异。
3. “核对实例配置”允许明确接纳同ID新配置行；角色仍须匹配，不允许换instance_id、跨节点、跨网关或角色转换。确认所用身份/指纹随预览返回，T05提交时重新检查。
4. 网关更新快照不自动改子设备IP；对子设备同时比较采纳的配置世代、规则key和host。仅在同世代下，未变规则和规则重排可继续有效；同ID新配置世代被父网关采纳后，孩子即使rule_key相同仍须逐条address采纳新来源，父确认不替代子确认。原规则删除/改变标待核对。
5. 子设备可在同网关当前规则内重新选择规则/host并明确确认地址差异，这属于地址修正，不开放归属移动。
6. 旧组首次补绑保留旧孩子地址/legacy模式。旧直属设备通过binding输出原新地址与字段影响；旧网关孩子（含self）通过instance-address预览及PUT address转换，不走独立实例binding。用户未确认保持原记录，旧IPv6不得自动丢弃。
7. 自身设备模式用网关实例overlay，LAN可空；网关实例地址变更后自身设备同样显示差异并逐条确认，不套普通子设备映射地址。
8. missing/unavailable等状态仍输出元数据编辑、服务CRUD、按旧规则删除的可执行动作；只限制依赖当前配置的绑定/派生地址操作。

## 依赖与交接

T01后可按类型契约独立实现逻辑并与T02/T03/T04并行；最终集成等待这些模块稳定。向T05交状态解析、首次绑定/核对差异和确认输入；向T07交接口所需只读预览；向前端交状态文案reason及动作矩阵。T05负责读取事务协调和全部写SQL，T06不另起资产写入路径。

## 待执行验收

legacy_unbound/current/changed/missing/replaced/unavailable各状态；离线但持久配置正常；同ID删除重建；同世代规则重排；新世代规则key相同仍需孩子确认；删除/新增/更换规则；网关确认后子地址不变；旧孩子经address转换；子规则修复；自身设备overlay差异；预览后再变更；查询前后数据库内容不变；失效状态仍能管理服务和元数据。

状态解析源码已实现；所列用例未执行，未运行测试、构建、CI或部署。

## 2026-10-23 实施交接

新增bindings.rs，区分未绑定、正常、变化、丢失、重建及不可读取；网关更新不改孩子地址，孩子独立确认来源。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
