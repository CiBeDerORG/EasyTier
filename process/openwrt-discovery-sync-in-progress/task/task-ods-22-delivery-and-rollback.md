---
id: ODS-T22
title: 交付、升级与回退记录
status: in_progress
owner: Codex
requirements: [ODS-R01, ODS-R02, ODS-R03, ODS-R04, ODS-R05, ODS-R06, ODS-R07, ODS-R08]
plan: PLAN-ODS-10
depends_on: [ODS-T21]
integration_dependencies: []
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T22 交付、升级与回退记录

- 所属计划：[PLAN-ODS-10 接线、验证与交付](../plan/plan-ods-10-integration-and-delivery.md)。
- 需求：[R01–R08 索引](../require/README.md)；具体验收按本任务范围与[公共契约](../plan/contracts/v1.md)关联。
- 状态：升级/回退文档已完成；发布、部署和实机验收未执行。

## 目标与边界

在用户后续授权交付时，为已验收提交整理升级、镜像、备份与回退记录。硬依赖T21；本次拆分不授权触发CI、发布或部署。本任务不扩大到LuCI插件改造，既有接口足够时只声明兼容版本。

## 输入输出与写入

输入验收记录、确定提交、目标部署架构与环境、当前工作流。拟拥有本需求 `task/delivery-record.md`、`task/rollback-record.md` 和必要部署说明，默认不改工作流。当前镜像工作流已支持feat/automation-center并忽略process文档；后续常规构建复用它，架构按当次部署要求，不默认全架构。

## 小步骤

列明结构/行为迁移及服务类型修正前置；备份数据库、运行配置、凭据所依赖的受管材料和执行器状态。核对Rust/前端/执行器withdraw与重连协议兼容，版本混用不能退回旧端口或整批取消。获得构建/发布授权后才执行并登记制品、摘要、架构、提交和真实结果；部署前停止受影响会话/任务并保留执行记录。回退同时考虑数据库结构与服务事件，不能仅拉旧镜像覆盖新库。

## 完成与交接

交付记录区分已构建、已发布、已部署及未执行，升级/回退步骤有对应备份与验证范围。用户只授权计划时，保持todo且不触发CI。不能把创建文档或推送文档等同于镜像已产出。


## 本轮实际交付

本轮源码、构建/验证证据、delivery-record.md 和 rollback-record.md 已准备；镜像发布部署未执行。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
