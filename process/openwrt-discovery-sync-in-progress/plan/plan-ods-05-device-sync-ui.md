---
id: PLAN-ODS-05
feature: openwrt-discovery-sync
title: 普通设备同步交互
status: ready
owner: null
requirements: [ODS-R02, ODS-R03, ODS-R08]
tasks: [ODS-T09]
---

# PLAN-ODS-05 普通设备同步交互

## 目标与边界

把 [ODS-R02](../require/require-ods-02-device-discovery-review.md) 的发现快照和 [ODS-R03](../require/require-ods-03-asset-apply-overwrite.md) 的用户选择呈现为可执行清单，结果遵循 [ODS-R08](../require/require-ods-08-results-and-acceptance.md)。首版支持历史记录、差异查看、逐条动作和服务保留选择，不扩展自动定时同步或跨网关批量操作。

## 小任务与先后

唯一任务为 [ODS-T09 同步差异清单与操作选择](../task/task-ods-09-discovery-review-ui.md)。硬依赖仅 T01 的公共契约，可与后端 T06/T07/T08 并行实现组件；T06、T07、T08 是实际联调前提，不能因使用契约示例展示成功就把后端接通记为完成。

顺序为：按契约建立展示模型 → 设备选择与差异行 → 覆盖服务勾选 → 逐条提交/失败恢复交互 → 交给 T19 接入现有页面。没有 IP 命中时的“手选旧设备”只列当前组非网关自身子设备；已有同组 IP 时禁用新建并显示原因。

## 独占范围与并行约束

T09 只新增 `frontend/src/components/node-assets/discovery/` 下的 `DeviceDiscoveryReview.vue`、`DiscoveryDiffTable.vue`、`ReplacementServiceSelection.vue`。这些均为拟新增文件。

网关基本信息和入口按钮属于 T05；ADB 专用结果属于 T12；共用页面、API 模块、类型、路由和翻译声明由 T01/T19 统一维护。T09 通过 props/emits 和公共适配边界交付，不抢写上述文件，避免组件各自持有一份网关或预览事实。

## 交互与交接

清单固定显示当前网关、来源/发现时间、历史标识、IP/MAC/名称差异、可用动作及不可用原因。覆盖时明确展示旧设备、将生成新记录、保留服务及已公开标记，逐项勾选，不隐藏未保留服务的删除后果。提交时锁定本次版本；失败只重试对应阶段，过期结果重新获取预览，关闭清单后不隐式提交。

向 T19 交付组件入口、事件、状态示例及未接线清单；向 T20 交付 ODS-R03-A01～A08 的可观察交互条件。当前未实施、未验证；本轮不新增或执行应用测试。
