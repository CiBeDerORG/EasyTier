---
id: T07
plan: P03
status: verified
depends_on: [T01, T02, T03, T04, T05, T06]
owner_role: asset-api-agent
updated_at: 2026-10-23
---

# T07 授权接口与后端组装

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。按[v2契约](../plan/contracts/v2.md)将实例候选、地址预览、旧资料补绑/核对与现有CRUD接为页面可调用的API，保持[后端](../require/require-02-backend-binding-and-addresses.md)及[生命周期](../require/require-04-lifecycle-and-compatibility.md)要求。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 唯一写入范围与入口

- 独占 `easytier-web/src/restful/assets/mod.rs` 的新增handler、query/body适配及router注册；必要REST私有新模块也由同owner写入。
- 复用 `authorize/scoped_context`、`machine_id`、JSON错误映射、`CentralNetworkService::lock_mutations()` 及领域层方法。
- 不改原登录/OIDC/config-server、`restful/network.rs`、中央发布、数据库结构或前端。`db/assets/mod.rs` 由T12统一接线。

## 输入与输出

HTTP具体路径、方法、请求体及错误码以v2为唯一准绳，不另起同功能不同命名。接口组应覆盖：实例候选、地址/绑定差异预览、网关/直属/子设备创建、普通资料更新、首次补绑、同实例核对、规则/host修正、列表/详情及既有删除。

本轮新增路由均位于 `/api/v1/assets`：`GET /network-instances`、`POST /validation/instance-address`、`POST /validation/binding`、静态组/设备两类 `PUT /{kind}/{id}/binding`、`PUT /devices/{id}/address`；kind不得拼接成任意表名。旧组/设备POST增加新输入，普通PUT区分legacy地址与新模式，具体请求形状与preview_version见v2。

输入始终携带或继承owner/node scope；输出只含资产所需精简数据、字段错误、状态reason和可执行动作，不返回完整网络配置。

## 实施内容

1. 所有新增读写路由先检查登录、external webhook可用性、当前actor是否可管理目标owner，再传递同一owner/node到领域层。管理员跨账号不能经旧网络HTTP丢失owner。
2. 候选/预览严格检查scope、角色、资源身份和输入字段；读取失败、实例不存在、无IPv4、角色不匹配、被其他网关占用分别返回契约可呈现原因。
3. 写handler持有现有mutation锁调用T05，HTTP层不再复制IP计算或另写SQL；新建和显式attach/refresh/address保留request_id与稳定用户请求语义。按领域结果返回成功/409/410/422，临时配置读取失败按契约503；运行RPC不在数据库写事务中长时间等待。
4. 新建的网关/普通入口与旧“管理网关组”入口必须调用相同后端约束，不能旧接口绕过实例必选。旧资料元数据编辑明确区分新建，不误阻断合法旧记录维护。
5. 旧网关/直属设备首次绑定与核对走binding，旧网关孩子（含self）转新模式走instance-address预览及PUT address，不单独绑定实例。不能借通用PUT隐式迁移、换角色或跨实例；预览只读、确认重读后保存，前端可以区分旧输入与最新配置冲突。
6. 列表/详情/创建/编辑统一使用T05响应序列化；状态来源一致，保存后页面能重新查询并定位。无效owner/node返回可恢复错误，不返回另一scope数据。
7. 保留服务路由、协议/端口规则、节点卡片实例数量与资产设备计数口径、旧节点删除保护。异常绑定不限制独立服务资料CRUD。
8. 限定按需请求，不注册新reconciler、轮询、定时任务或网络写操作；不把保存资产成功描述为真实通信已配置。
9. 新网关binding预览后POST创建，其他新设备instance-address预览后POST创建，单次POST原子提交资料与绑定/地址。一般元数据保存、绑定核对、地址修正分别是独立明确操作，不能让多个成功/失败混合的请求被汇总成一次原子成功。

## 依赖、并行与交接

依赖T02–T06领域交接，T01后可先按契约准备薄handler但不重写未交付业务逻辑。REST文件唯一owner避免CRUD各agent冲突；向T08提供客户端类型/调用样例，向T12交路由清单及导出依赖，向T13交授权/错误/兼容用例。最终统一构建/测试由T13授权执行，本任务不自行扩大验证范围。

## 待执行验收

普通账号自身、普通账号跨owner拒绝、管理员跨owner、external webhook拒绝；篡改node/instance/group/设备ID；同名候选辨识；无效JSON/query字段；401/403/404/409/410/422映射；离线查询；前端原表单与新入口使用同一规则；原实例/服务API无行为变化。

当前仅文档，未修改或运行handler、测试、构建、CI、部署、push。

## 2026-10-23 实施交接

资产REST增加候选、预览、绑定与地址路由；授权后按需获取有界运行信息，在写锁/事务外完成；元数据/服务/删除不依赖RPC。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
