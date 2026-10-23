---
id: T05
plan: P03
status: verified
depends_on: [T01, T02, T03, T04, T06]
owner_role: asset-domain-agent
updated_at: 2026-10-23
---

# T05 资产写入、首次绑定与地址保存

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。将[后端需求](../require/require-02-backend-binding-and-addresses.md)和[生命周期规则](../require/require-04-lifecycle-and-compatibility.md)接入现有CRUD，遵守[v2契约](../plan/contracts/v2.md)。本任务是领域写入唯一负责人。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 唯一写入范围与基线

- 独占 `easytier-web/src/db/assets/common.rs`、`validation.rs`、`groups.rs`、`devices.rs`、`idempotency.rs` 的本轮修改；有必要的新领域写入文件归同一owner。
- `groups.rs::create_group/update_group` 当前收手填subnets；`devices.rs::create_device/update_device` 当前收手填IP并固定group；`idempotency.rs::Operation` 已有请求哈希与重放；`common.rs::group_json/device_json` 是所有读写响应共有入口。
- 不修改迁移、HTTP、T03/T04/T06独占helper、旧网络写入或前端。公共模块注册提交交接说明，由T12接线。

## 输入与输出

输入为已授权actor/owner/node及契约区分的新建、普通资料编辑、首次绑定、同实例核对、子规则/host修正请求。调用T03/T04/T06读取和推导。输出持久资产、绑定精简快照、地址状态、可执行动作以及统一字段错误/冲突响应；正式响应与列表/详情必须相同语义。

## 实施步骤

1. 新建网关必须选择当前节点有IPv4代理/映射规则的持久实例，组IP/规则取自该实例；采纳快照时同步更新asset_gateway_subnets中的去重real网段投影。新直属设备绑定无子网转发实例。用户角色先选，服务器再验证匹配。
2. 普通网关子设备只能在所属网关下选规则和host offset，服务器生成两侧IP；自身设备overlay来自网关实例，LAN可空或由real侧规则生成。保留固定归属和至多一条自身设备。
3. 扩展严格输入白名单和规范化，拒绝跨scope实例、任意生成IP、未知角色/模式、不可读配置及缺少必选绑定的新增；保留旧资料一般编辑，不把旧记录强制转换为新流程。
4. 用T06差异结果实现首次补绑及同实例核对：旧网关/直属设备通过PUT binding确认；旧网关孩子（含self）通过instance-address预览及PUT address转换，不能独立选择实例。请求明确携带预览身份/指纹与确认范围，不自动换instance_id、角色、节点或网关，不对未确认的旧子设备批量重算，旧IPv6不静默丢弃。
5. 同网关子设备可重新选择当前有效规则/host并确认原新地址。同世代未变规则可继续使用；父网关采纳同ID新配置世代后，即使规则key相同，孩子仍需逐条address确认新来源。网关实例IPv4改变时self的旧overlay也待核对；父快照更新不自动重算任何孩子IP。
6. 保存时重新读取配置身份/指纹、实例角色和IP来源，重算地址。相关配置已变化时409并返回可恢复原因；不能静默接受过期预览。元数据最后保存生效与绑定指纹保护分别处理。
7. 在现有写锁协调与BEGIN IMMEDIATE事务中检查并提交资产/快照/地址及幂等结果；数据库唯一约束保障网关绑定、LAN与自身设备竞争。运行RPC不能置于数据库写事务中长时间等待，持久来源与预览版本在最终提交前重查。
8. 新建及显式attach/refresh/address请求的幂等哈希基于规范化用户请求、preview_version和scope，不含再次读取会变化的运行IP/快照。授权后先重放，再做新操作的占用/配置检查；相同请求返回原结果，改变请求409，原结果被删410。复用Operation时扩展操作名以隔离显式变更，普通元数据PUT保持既有最后保存语义。
9. 统一group_json/device_json中的精简绑定状态/地址来源，不返回完整NetworkConfig；节点计数仍只数asset_devices，服务所属与删除级联保持。
10. 删除资产清理其内联绑定，释放网关实例占用；不调用真实网络删除/停用/转发接口。节点/网关非空禁删、设备删除提交时全部服务继续有效。
11. 新网关binding预览→单次POST、其他新设备instance-address预览→单次POST，资料与绑定/地址在一次事务中创建。已有资料的元数据PUT、binding PUT、address PUT各自独立返回结果，不让前端串联后统一声称全部原子成功。

## 异常状态下可操作范围

legacy_unbound/changed/missing/replaced/unavailable均保留旧名称、地址、服务和可审阅状态；一般名称/型号/备注/类型修改、服务CRUD和原删除流程不能被实例读取失败整体禁用。legacy_manual一般资料编辑仍可维护原手填地址；新模式的派生地址只能经独立address确认分支修改。当前不可确定配置时给出原因并保留输入，详细动作矩阵由v2与T06统一。

## 依赖与交接

T01–T04及T06交接后接入；可提前按契约准备输入/响应代码，但不得复制他们的helper逻辑。所有上述共享文件由同一agent顺序修改；向T07提供领域方法和错误样例，向T08/T12提供读写响应/定位行为，向T13交事务与竞争场景。独占文件中的测试补充由本owner或后续授权验证owner串行接手，不同时写。

## 待执行验收

- 原址/映射网关、直属、网关普通子设备、自身设备及legacy补绑全路径。
- 伪造地址、角色不匹配、跨账号/节点、并发网关占用、同节点LAN冲突、自身设备重复。
- 创建及attach/refresh/address超时重试、重复请求时配置变化、相同request_id改内容、删除后重试。
- 同ID配置行替换后父确认仍保留孩子旧来源，规则key相同也需孩子逐条address确认；同世代规则重排不会误报，网关IP变self待核对。角色变化不得强行接受，子规则修正后地址正确。
- 失效绑定仍可元数据/服务管理和删除；计数、固定归属、服务协议/端口唯一及级联保持。

上述源码已实现；所列用例未执行。调试模式下未运行测试、构建、CI或部署。

## 2026-10-23 实施交接

新增binding_ops.rs并更新资产CRUD与校验；创建、补绑、核对及地址修改在事务内复核预览，保留幂等重放和固定归属。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
