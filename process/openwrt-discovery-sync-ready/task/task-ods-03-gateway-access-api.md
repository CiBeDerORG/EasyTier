---
id: ODS-T03
title: 网关接入配置与授权接口
owner: null
status: todo
depends_on: [ODS-T02]
integration_dependencies: [ODS-T04]
requirements: [ODS-R01]
plan: PLAN-ODS-02
---

# ODS-T03 网关接入配置与授权接口

所属[需求 ODS-R01](../require/require-ods-01-gateway-access.md)及[计划 PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md)。ODS-T04 仅阻塞联调，不阻塞配置读写的独立开发；所需共享契约通过 ODS-T02 的前置 ODS-T01 获取。

## 输入与输出

输入为网关自身设备 ID、现有资产权限上下文、接入配置及密钥变更意图。拟新增接口前缀为 `/api/v1/assets/discovery/gateways/{gateway_device_id}`，实现 `GET/PUT /access`、`POST /access/check` 的处理器；字段只采用[公共契约 v1](../plan/contracts/v1.md)。输出脱敏配置、配置状态及分层检查结果，内部向发现客户端提供凭据。

## 独占写入与非写入范围

- 拟新增 `easytier-web/src/db/discovery/gateway_access.rs`：配置读取/保存、凭据保留和内部解析。
- 拟新增 `easytier-web/src/restful/discovery/gateway_access.rs`：归属/角色检查、配置与检查处理器。
- 不改数据库迁移、实体、模块注册、共享 DTO、总路由、ApiClient、LuCI 客户端或任何页面；分别交给 ODS-T02、ODS-T01/ODS-T19、ODS-T04。

## 可执行步骤

1. 使用 T02 字段查询发起设备，验证用户/节点权限、当前组、网关自身角色及 OpenWrt 类型语义。
2. 规范化可修改地址，未配置时生成约定默认 URL；按 `secret_action` 保留、替换或明确清除密钥，清除不删除已有资产，明文不回传。
3. 使用 `expected_revision` 检查并发配置变更；读取和保存均不扫描，更新后使旧配置上下文失效。
4. 按契约调用 T04 检查登录与固定能力，不执行普通/ADB 扫描；将认证、权限、网络和证书错误映射为可读状态，不泄露原始登录响应。

## 完成条件与交接

达到 ODS-R01-A01～A05；同一用户其他网关也不得通过替换组参数误用凭据。向 ODS-T05 交付脱敏表单接口，向 ODS-T06/ODS-T10 交付内部配置解析，向 ODS-T19 交付待注册处理器和错误码。

## 验证计划与代码依据

后续覆盖越权/错组/错角色、空密钥保留、URL 变更、明文泄漏、检查失败与无隐式扫描。**本轮未执行：仅文档拆分，不运行测试、构建或路由器请求。**

现有字段依据：[asset_devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/entity/asset_devices.rs)；认证依据：[LuCI AUTHENTICATION.md](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/docs/AUTHENTICATION.md)。
