---
id: T03
plan: P02
status: verified
depends_on: [T01]
integration_dependencies: [T02, T04]
owner_role: instance-catalog-agent
updated_at: 2026-10-23
---

# T03 作用域实例目录与地址来源

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。实现[后端需求](../require/require-02-backend-binding-and-addresses.md)中的只读配置目录，按[v2契约](../plan/contracts/v2.md)提供统一DTO。T01后可与T02/T04并行，最终接入依赖其结构/算法交接。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 源码基线与唯一写入范围

- 独占新文件 `easytier-web/src/db/assets/instances.rs`；相关内部类型也在本文件或T01指定的新独占文件。
- 复用 `db/mod.rs::list_network_configs/get_network_config`、`db/entity/user_running_network_configs.rs`；通过已有中央成员资料读取allocated IPv4，必要时按需取已有运行信息。
- 不改资产CRUD、HTTP、旧 `restful/network.rs`、Core或配置写路径；模块声明交T12。

旧 `management/full/remote_client.rs::handle_list_network_instance_ids` 要求在线；`handle_get_network_config_with_source` 运行优先。新目录不能直接调用二者并宣称实现持久配置优先、离线可管理。

## 输入与输出

输入：已授权owner/node、期望角色、必要时当前资源ID以排除自己的占用。输出：持久实例身份、名称、IPv4及来源、规范规则与相关指纹、选择状态/原因。对绑定核对另提供按同scope实例ID重读的内部入口。字段名与错误枚举以v2为准。

## 实施内容

1. 以当前owner/node的持久配置All为基础，包含disabled；校验节点scope，不能使用nil machine ID意外列出全账号配置。
2. 不因离线或停用直接排除配置。运行独有无持久行不允许绑定，返回/展示原因；仅此条件不创建持久配置。
3. 绑定身份包含实例ID、持久行ID与create_time；相关指纹只计算本需求必要字段并规范化规则顺序，避免无关密钥或规则重排造成错误差异。
4. IPv4来源优先静态配置，再可对应中央成员allocated IPv4，最后已知运行IPv4辅助；不得把中央网络地址池、节点公网来源IP或手工资产IP当作该实例IPv4。
5. IPv4缺失/配置解析失败提供不可新绑定原因；仅IPv6规则或未知格式不能误归直属。新IPv6推导后置，原配置不改。
6. 角色依据IPv4子网代理/映射规则。有任一规则属网关，无子网转发属直属；调用T04统一解析，不独立复制地址算法。
7. 查询绑定占用时使用owner/node/instance范围，排除正在核对的自身资源；网关唯一按T02/T05保障，不擅自对直属新增排他限制。
8. 仅返回精简字段，不输出NetworkConfig、network secret或凭据。来源读取失败和实例真正不存在区分，供T06形成状态。

## 边界与交接

只读，无持久化快照、无规则写入、无自动探测/定时查询。HTTP权限由T07负责，内部scope检查不能省略；管理员选择owner必须一路传递，不能回落登录账号。

向T05交新建保存时重读入口；向T06交身份/指纹及missing/unavailable依据；向T07交候选DTO、字段错误与可选原因。新方法签名、错误样例和纯DTO在T01契约中固定，模块注册由T12完成。

## 待执行验收

当前/其他账号与管理员scope、同ID跨节点、离线持久实例、disabled实例、运行独有实例、DHCP无可用IPv4、静态/分配/运行来源优先级、仅IPv6或异常规则、相同名字的实例、同网关占用与自己核对、无凭据泄漏。候选读取不能修改配置或资产数据。

本轮不执行这些用例，不运行测试、构建、CI、服务或部署。

## 2026-10-23 实施交接

新增instances.rs，按账号与节点读取持久配置、中央分配及可选运行观察；包含停用配置，匹配Core的DHCP默认语义，输出精简摘要。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
