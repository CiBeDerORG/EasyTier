---
id: P03
status: completed
depends_on: [P01, P02]
tasks: [T05, T06, T07]
updated_at: 2026-10-23
---

# P03 后端保存、生命周期与授权接口

**最新状态：** 本计划已完成对应开发、授权验证及当前云环境交付；实际结果和未覆盖项见[验证报告](../task/verification-report.md)与[交付记录](../task/release-record.md)。以下验收设计保留，不将未实测范围扩大为已通过。

目标：将实例候选、地址预览、正式保存、旧资料补绑和实例变化处理连成完整后端流程，供现有节点详情页调用。依据[后端需求](../require/require-02-backend-binding-and-addresses.md)、[生命周期需求](../require/require-04-lifecycle-and-compatibility.md)及[v2契约](contracts/v2.md)。源码已接入，未运行测试。

## 小任务与代码所有权

|任务|唯一负责人|独占范围|前置交接|
|---|---|---|---|
|[T06 生命周期解析](../task/task-06-binding-lifecycle.md)|binding-lifecycle-agent|新 `db/assets/bindings.rs`，按需状态与差异预览|T01；T02/T03/T04最终接口|
|[T05 资产写入](../task/task-05-asset-writes-and-binding.md)|asset-domain-agent|`db/assets/common.rs`、`validation.rs`、`groups.rs`、`devices.rs`、`idempotency.rs`|T01–T04；T06解析/确认输入输出|
|[T07 授权HTTP](../task/task-07-authorized-api.md)|asset-api-agent|`restful/assets/mod.rs`|T02–T06稳定输出及契约|

T06可以在T01确定接口后与基础模块并行编写独立逻辑，最终依赖完成后交接；T05负责所有领域写入，不让生命周期模块另建一套写SQL。T07可先按契约准备薄handler，最终接入等待领域输出。`db/assets/mod.rs` 由T12统一注册，以上agent提交模块导出清单或补丁说明，不共同编辑。

## 端到端路径

1. 授权后的候选查询限定owner/node/角色，返回可读名称、精简实例配置、来源和选择原因。
2. 新网关调用binding预览，其他新设备调用instance-address预览；预览只读，保留前端输入，随后单次POST原子保存资料及绑定/地址。
3. 正式创建先鉴权、检查幂等重放，再检验持久行身份、相关指纹、角色、占用和地址；事务提交资产、绑定/地址、幂等结果。
4. 旧网关/直属设备通过binding补绑或核对；旧网关孩子（含self）通过address转换，不单独绑定实例。同ID实例新配置世代须显式核对，父网关采纳不替代孩子逐条address确认，即使规则key相同；网关IPv4变化时自身设备overlay也待核对。
5. 查询按需计算legacy_unbound/current/changed/missing/replaced/unavailable并提供可执行动作；失败状态仍允许普通元数据、服务和既有删除流程。
6. 实例删改继续原操作；删除资产只释放资产绑定，原服务归属/级联与计数口径保持。已有资料的元数据、绑定、地址采用分别明确的操作，不串联多个提交后假报一次原子成功。

## 保留约束

不改config-server/OIDC、旧实例写入流程和中央网络发布；不把候选读取包装成网络控制动作；不新增定时刷新。普通字段仍最后保存生效；绑定确认采用相关配置指纹，未授权移动/换角色/跨实例绑定。完整配置含secret不进入响应或快照。

## 交接与验收

T07向T08–T12提供真实契约形状与字段错误/冲突样例；T05向T13交事务/幂等/唯一性场景；T06向前端交状态与动作矩阵；T12统一接线后再由T13按授权开展集中验证。

验收清单包括角色匹配、伪造/跨scope输入、同节点抢占、重试原结果、旧规则消失后修正、删除重建的实例身份、元数据和服务不被异常绑定锁死、读取无副作用、原实例操作无新限制。本文列出待验范围，不将历史CRUD验证视为本计划通过。
