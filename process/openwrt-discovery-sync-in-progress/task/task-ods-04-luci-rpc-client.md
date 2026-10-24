---
id: ODS-T04
title: LuCI RPC 认证与发现客户端
owner: Codex
status: done
depends_on: [ODS-T01]
integration_dependencies: [ODS-T03]
requirements: [ODS-R01]
plan: PLAN-ODS-02
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T04 LuCI RPC 认证与发现客户端

所属[需求 ODS-R01](../require/require-ods-01-gateway-access.md)及[计划 PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md)。ODS-T03 仅阻塞接入真实配置的联调；可用注入配置和传输替身独立开发。

## 输入与输出

输入为内部解析后的 URL/凭据及固定发现能力。输出规范化 RPCD 结果、原快照时间、来源状态及明确错误阶段。接口和 DTO 遵循[公共契约 v1](../plan/contracts/v1.md)，不对外返回会话令牌。`scan` 已带当次结果，不额外调用 `list` 冒充新结果。

## 独占写入与非写入范围

唯一拟新增文件为 `easytier-web/src/discovery/luci_client.rs`，包含客户端、登录、固定方法调用及响应解析符号。不得修改凭据存储、资产事务、共享类型、Cargo 依赖清单、模块注册或 LuCI 仓库；需要共享依赖时提交给 ODS-T01/ODS-T19。

## 可执行步骤

1. 实现固定 `easyadb` 登录，每次显式操作获取会话；不跨操作依赖旧会话，只对明确失效最多重新登录一次，权限不足不循环重试。
2. 除登录与固定能力查询外，仅开放 `easy-discovery.devices`、`easy-discovery.adb` 的 `list/scan` 枚举，不接受任意 ubus 对象、命令或网段；接入检查不得以实际扫描代替能力检查。
3. 逐层解析 HTTP、JSON-RPC、ubus 状态和业务 `ok/partial/error`；保留旧快照、`has_snapshot`、截断及来源错误，不凭数组非空认定成功。
4. 设置有界响应和扫描超时，扫描窗口之外留足 RPC 开销，端到端超时不少于 15 秒；禁止静默降级或关闭证书校验，错误输出不包含密钥/会话。

## 完成条件与交接

满足 ODS-R01-A03/A04；四项能力行为独立，登录与发现错误可区分，读取不会扫描。向 ODS-T03 提供检查适配，向 ODS-T06/ODS-T10 提供发现调用，向 ODS-T19 交付注册需求与响应样例。

## 验证计划与代码依据

后续用模拟响应验证会话过期、ACL 拒绝、损坏 JSON、超时、证书失败、部分成功和失败携带旧快照。**本轮未执行：只编写任务，不测试或构建。**

协议依据：[LuCI docs/API.md](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/docs/API.md)；实际固定入口：[rpcd/easy-discovery.devices](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/libexec/rpcd/easy-discovery.devices)及同目录 `easy-discovery.adb`。

## 本轮实际交付

HTTPS /ubus 客户端已实现，loopback TLS LuCI 模拟认证/扫描通过。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
