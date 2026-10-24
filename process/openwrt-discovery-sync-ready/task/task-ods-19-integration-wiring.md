---
id: ODS-T19
title: 共享入口接线与全链路复审
status: todo
owner: null
requirements: [ODS-R01, ODS-R02, ODS-R03, ODS-R04, ODS-R05, ODS-R06, ODS-R07, ODS-R08]
plan: PLAN-ODS-10
depends_on: [ODS-T05, ODS-T08, ODS-T09, ODS-T10, ODS-T11, ODS-T12, ODS-T13, ODS-T14, ODS-T15, ODS-T16, ODS-T17, ODS-T18]
integration_dependencies: []
---

# ODS-T19 共享入口接线与全链路复审

- 所属计划：[PLAN-ODS-10 接线、验证与交付](../plan/plan-ods-10-integration-and-delivery.md)。
- 需求：[R01–R08 索引](../require/README.md)；具体验收按本任务范围与[公共契约](../plan/contracts/v1.md)关联。
- 状态：未认领、未开始；文档编写不代表实施。

## 目标与边界

在各模块交付后由唯一集成人完成共享入口接线和全链路源码复审。输入各任务的模块、契约、未测项及接线请求；输出能串联完整业务的统一实现与缺口清单。本任务不把未授权测试或真实设备未验证写成通过。

## 独占写入与步骤

负责Rust crate/db/restful/discovery模块注册、AppState/配置注入、migrator/mod.rs、既有assets/automation公共路由和validation适配、前端公共DTO/ApiClient/NodeAssetsPage/AssetDeviceTable与既有设备表单嵌入、executor app.py/validation.py和协议能力登记。T01已写骨架仅本任务后续顺序修改；不抢写业务模块。先核对所有输入/输出，注册迁移000019/000020；连接网关设置/预览/应用/ADB结果；把普通更新和手工服务修改纳入同一端点事件；接上公开apply、withdraw、重连和原画面恢复。检查缺执行器能力时明确报错，禁止退回整批取消或旧端口。

## 完成条件与交接

沿R01–R08逐项追踪每个入口到持久化与外部动作；无未接入按钮、双端口来源、错组写入、旧会话续跑或共享文件遗漏。保留/删除服务两路都交代公开关系及历史。给T20具体提交、实际可用验证命令与未完成项，给T21真实环境前置清单。

## 后续验证

这里只确认源码接线交付；执行测试需T20授权条件。用户未要求实现时，本任务保持todo，本轮未改任何共享源码。
