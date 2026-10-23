---
id: T04
plan: P02
status: verified
depends_on: [T01]
owner_role: ipv4-agent
updated_at: 2026-10-23
---

# T04 IPv4规则规范化与主机地址推导

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。依据[后端地址需求](../require/require-02-backend-binding-and-addresses.md)与[v2契约](../plan/contracts/v2.md)。本任务提供纯函数，不接数据库/HTTP，不修改真实网络。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 唯一写入范围和代码依据

- 独占新文件 `easytier-web/src/db/assets/address_derivation.rs`；模块注册交T12。
- 只读参考 `easytier-core/src/config/api_input.rs::add_proxy_network_to_config`、`config/toml.rs` 的映射前缀约束、`gateway/proxy/cidr_table.rs::ProxyCidrEntry::lookup_v4`。
- 不改Core、不改现有资产validation文件；需要的调用点由T05接入。

## 输入与输出

输入为实例子网规则字符串、IPv4实例地址、选中规则key及整数host offset；按调用场景区分规则解析、普通子设备和网关自身设备推导。输出规范化real/mapped CIDR、稳定规则key、规则模式、LAN/组网侧访问IP及结构化字段错误/提示。完整签名与错误码以v2为准。

## 实施内容

1. 支持单CIDR原址代理和 `real_cidr->mapped_cidr`；映射两侧前缀一致，保留主机位，不能反转real/mapped。
2. 规范化IPv4网段及规则序列用于稳定标识；规则key不得使用数组下标。坏格式、非法前缀、仅IPv6及混合异常明确返回原因，不当作空规则。
3. host offset为相对网段起点的整数，范围0至网段地址数减1；计算使用足够宽的整数防止/0或移位溢出。支持/23、/25等非/24，/31的0和1、/32的0。
4. 对前缀不大于30的网络地址/广播地址给warning但允许登记，不宣称这些地址实际可达；界面“最后一位”只是/24的表现形式。
5. 普通子设备返回real+offset作为LAN，mapped（若无则real）+offset作为组网侧访问IP。自身设备overlay使用网关实例IPv4；其LAN可空，填写时规则/offset只生成real LAN。
6. 预览和正式保存共用函数；不使用客户端传入的已计算IP作为真值，不进行DNS查询、探测、分配、路由或端口转发。
7. 地址/规则错误与网络可达性分别表达。LAN同节点唯一、自身设备唯一、绑定占用由T05/T02负责，纯函数不模拟数据库状态。

## 交接与并行

T01后可与T02/T03独立并行。向T03交解析、规范规则和指纹所需数据；向T05/T06交生成与差异结果；向前端提供/v2中/24与非/24示例及字段错误，不让前端独立决定最终IP。模块导出清单交T12，不共同编辑mod.rs。

## 待执行验收示例

|输入|预期|
|---|---|
|原址 `192.168.1.0/24`、20|两侧均为192.168.1.20|
|`192.168.1.0/24->10.20.1.0/24`、20|LAN为192.168.1.20，访问IP为10.20.1.20|
|`192.168.0.0/23->10.20.0.0/23`、276|LAN为192.168.1.20，访问IP为10.20.1.20|
|/31 offset0/1；/32 offset0|范围内正确生成，不误判越界|
|/24 offset256、负数或小数；不等前缀|字段错误，不截断或自动改输入|
|/24 offset0/255|保留地址提示，允许登记|
|规则重排与IPv4规范表达等价|稳定key/指纹语义一致|
|网关自身overlay10.10.0.1，选映射规则host20|overlay仍10.10.0.1，LAN为real侧结果|

用例仅作为后续授权验证清单，当前未执行测试、构建或CI。

## 2026-10-23 实施交接

新增address_derivation.rs，解析原址代理/网段映射、稳定规则键及/0–32主机偏移；自身设备overlay来自实例。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
