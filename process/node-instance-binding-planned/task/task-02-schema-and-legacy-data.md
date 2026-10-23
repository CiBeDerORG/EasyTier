---
id: T02
plan: P02
status: verified
depends_on: [T01]
owner_role: schema-agent
updated_at: 2026-10-23
---

# T02 增量结构与旧资料保留

状态：已实现，真实旧库升级和数据保留验证通过，见[验证报告](verification-report.md)。新增000013迁移并注册，两张资产实体增加绑定字段；旧资料不回填猜测值。依据[后端需求](../require/require-02-backend-binding-and-addresses.md)、[生命周期需求](../require/require-04-lifecycle-and-compatibility.md)及[v2契约](../plan/contracts/v2.md)。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 当前入口与唯一写入范围

- 基线迁移：`easytier-web/src/migrator/m20261023_000012_node_assets.rs`；新增独立迁移，不回改已执行历史迁移。
- 独占必要的 `easytier-web/src/db/entity/asset_gateway_groups.rs`、`asset_devices.rs`，以及实体/迁移注册文件。保留两表、ID、节点范围、网关归属和服务外键。
- 不改 `db/assets/*` 领域文件、HTTP、原配置迁移或前端；`db/assets/mod.rs` 的注册交T12。

## 输入与输出

输入为T01冻结的字段、状态及模式契约；输出为增量迁移、对应实体字段、DDL/索引说明、升级前后字段对照及T05/T06可使用的数据结构。

采用内联绑定身份标量和精简JSON快照：组/直属设备保存实例作用域引用、配置行身份和相关指纹，子设备保存地址模式、规则key、host offset与既有派生IP。地址模式按v2区分legacy_manual/direct_instance/gateway_rule/gateway_self；未绑定状态名为legacy_unbound。具体字段名以v2为准，不另建第二套绑定表。

## 实施内容

1. 增量字段允许旧记录未绑定；旧组、旧设备、旧服务、时间戳、名称/IP及IPv6值原样保留，不猜测实例或批量计算新地址。
2. 明确legacy识别条件，避免部分字段空值形成“已绑定”假象；新建必选由T05严格检查，不能因DDL nullable接受新手工绕过路径。
3. 为网关实例建立owner/node/instance作用域唯一索引，允许不同节点相同instance_id；直属不增加实例排他唯一。
4. 保留同节点非空LAN唯一、每网关自身设备最多一条、外键scope及设备级联服务。JSON快照不承载这些需数据库保障的唯一键。保留asset_gateway_subnets兼容表，用于已采纳real网段的去重投影，旧记录原值保留至明确补绑。
5. 配置行ID/create_time仅用于身份核对，不引入会拦截原配置删除或级联删除资产的强外键；资产关系继续由既有外键保护。
6. 精简快照只含实例IP来源和IPv4规则等必要值；不复制NetworkConfig、secret、私钥、凭据。
7. 提供升级/回退注意事项；保留资产数据的恢复使用备份，不能将删列或回滚造成的数据丢失包装成无损操作。

## 依赖、交接与并行

T01完成后可与T03/T04并行。唯一schema-agent写迁移、实体及其注册；向T05交字段/NULL语义和约束，向T06交身份/快照读取结构，向T13交隔离旧库验收准备说明。迁移文件名按现有最大编号确定，避免多agent各自创建同号迁移。

## 待执行验收

- 有组、直属、子设备、自身设备、服务和IPv6资料的旧库升级，ID/归属/原IP/服务关系不变，旧记录可识别为legacy。
- 两个节点绑定相同instance_id合法；同节点两个网关重复绑定由唯一索引拒绝；直属不错误套用该限制。
- 原LAN/自身设备/服务端口外键及唯一约束仍有效；配置行删除不删除资产也不被新外键拦截。
- 字段缺失/局部绑定不能被新创建接口当成有效绑定；快照无完整配置和凭据。

以上为T13授权后的验收用例，本轮未执行迁移、测试、构建、CI、部署或push。

## 2026-10-23 实施交接

新增000013迁移并注册，更新两张资产实体；只新增绑定列、模式及网关实例唯一索引，未执行迁移。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
