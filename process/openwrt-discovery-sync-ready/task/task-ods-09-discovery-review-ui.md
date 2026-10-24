---
id: ODS-T09
feature: openwrt-discovery-sync
title: 同步差异清单与操作选择
owner: null
status: todo
depends_on: [ODS-T01]
integration_dependencies: [ODS-T06, ODS-T07, ODS-T08]
requirements: [ODS-R02, ODS-R03, ODS-R08]
plan: PLAN-ODS-05
---

# ODS-T09 同步差异清单与操作选择

所属：[ODS-R02](../require/require-ods-02-device-discovery-review.md)、[ODS-R03](../require/require-ods-03-asset-apply-overwrite.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)；计划：[PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md)。可在 T01 后独立开发，真实联调等待 T06/T07/T08。

## 输入与输出

输入为当前网关上下文、发现预览、组内候选设备及服务列表。输出逐条动作、手选目标和保留服务选择；版本、提交与结果结构引用[公共契约](../plan/contracts/v1.md)，不自行推断后端成功。

## 独占拟改范围

仅新增 `easytier-web/frontend/src/components/node-assets/discovery/` 下 `DeviceDiscoveryReview.vue`、`DiscoveryDiffTable.vue`、`ReplacementServiceSelection.vue`。拟新增组件名即交接入口。只读 `AssetDeviceTable.vue` 的展示约定及公共类型。T05授权/入口、T12ADB结果、共享页面、API模块、类型、路由和翻译由各自负责人处理。

## 小步骤

1. 展示固定目标网关组、发现时间/历史状态、IP/MAC/发现名称和已有资料差异；不把缓存记录标成当前在线。
2. 每项提供允许的动作。同组占IP禁用新建；未命中可新建或手选同组非网关自身。显示不可执行原因，不把“跳过”变成删除。
3. 覆盖前展示旧设备及将生成新记录，列出服务和公开标记供逐项勾选；明确未保留项将删除，保留项随新设备迁移。
4. 锁定提交版本，逐项显示成功/冲突/失败；过期项重新取预览，其他已完成项不重放。关闭页面不隐式提交，API失败后保留可恢复选择。

## 完成标准与未来验证

覆盖 ODS-R03-A01～A08 的用户可见部分。未来验证无匹配、同IP冲突、手选、全部/部分服务保留、过期预览、部分失败及重复点击。契约样例只验证组件交互，不能替代真实后端联调；本轮应用测试、构建和CI均未执行。

## 交接

向 T19交付组件props/emits、异步状态和所需翻译键清单，公共接线由T19统一修改；向T20交付交互验收场景。该组件不发起ADB专用结果呈现，不新增授权密码状态副本。
