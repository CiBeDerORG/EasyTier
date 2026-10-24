---
id: ODS-T05
title: 网关基本信息与发现操作入口
owner: Codex
status: done
depends_on: [ODS-T01]
integration_dependencies: [ODS-T03]
requirements: [ODS-R01]
plan: PLAN-ODS-02
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T05 网关基本信息与发现操作入口

所属[需求 ODS-R01](../require/require-ods-01-gateway-access.md)及[计划 PLAN-ODS-02](../plan/plan-ods-02-gateway-access.md)。先按冻结接口开发独立子组件；ODS-T03 只阻塞真实保存/检查联调。

## 输入与输出

输入为网关自身资产、当前组、类型语义、脱敏配置及操作状态。输出配置保存/显式检查请求，以及携带网关设备上下文的“立即同步”“ADB 发现”事件。属性、事件和字段遵循[公共契约 v1](../plan/contracts/v1.md)，事件不得携带明文密钥。

## 独占写入与非写入范围

- 拟新增 `easytier-web/frontend/src/components/node-assets/GatewayDiscoveryAccess.vue`：基本信息中的 URL、授权密码及配置状态。
- 拟新增同目录 `GatewayDiscoveryActions.vue`：设备行的两种发现入口及忙碌态。
- 不改 `AssetDeviceDialog.vue`、`AssetDeviceTable.vue`、`NodeAssetsPage.vue`、页面 composable、共享类型、ApiClient 或路由；ODS-T19 统一挂载。普通差异清单由 ODS-T09 写，ADB 结果由 ODS-T12 写。

## 可执行步骤

1. 根据网关自身角色与 OpenWrt 类型显示配置区/入口，默认 URL 使用传入的组网 IPv4，保留已保存自定义地址。
2. 已存密钥仅显示已配置状态；空输入表示保留，清除用明确操作生成契约中的清除意图；临时明文在关闭/切换上下文后清理，不进入浏览器持久存储。
3. 实现保存和登录/能力检查的错误/忙碌状态，检查不扫描，不能把保存成功显示为连接成功。
4. 两个行按钮仅发出当前网关事件；防止重复点击，切换设备后丢弃旧响应，打开页面不主动扫描。

## 完成条件与交接

满足 ODS-R01-A01/A02/A04/A05。组件能使用契约替身独立演示；向 ODS-T19 提供挂载位置、props/emits、禁用条件和上下文失效规则，向 ODS-T09/ODS-T12 交付入口事件约定。

## 验证计划与代码依据

后续覆盖角色可见性、默认 URL、空密钥保留、错误阶段、切换设备和重复点击。**本轮未执行：仅任务文档，不运行前端测试、类型检查或构建。**

现有界面依据：[AssetDeviceDialog.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/node-assets/AssetDeviceDialog.vue)；现有资产字段：[types/node-assets.ts](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/types/node-assets.ts)。

## 本轮实际交付

设备类型语义确认、网关自身入口及接入对话框已实现，桌面/移动验证通过。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
