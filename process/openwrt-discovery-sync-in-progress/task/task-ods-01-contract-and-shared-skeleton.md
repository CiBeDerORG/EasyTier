---
id: ODS-T01
title: 公共契约冻结与共享入口骨架
status: done
owner: Codex
requirements: [ODS-R01, ODS-R02, ODS-R03, ODS-R04, ODS-R05, ODS-R06, ODS-R07, ODS-R08]
plan: PLAN-ODS-01
depends_on: []
integration_dependencies: []
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T01 公共契约冻结与共享入口骨架

- 所属计划：[PLAN-ODS-01 公共契约与持久化基础](../plan/plan-ods-01-contracts-and-storage.md)。
- 需求：[R01–R08 索引](../require/README.md)；具体验收按本任务范围与[公共契约](../plan/contracts/v1.md)关联。
- 状态：代码已完成，验证证据见[测试记录](test-report.md)。

## 目标与边界

冻结可执行的公共契约及共享类型骨架，让各模块按同一身份、字段和事件协作。输入为八个已确认需求、当前源码、LuCI API；输出为[公共契约v1](../plan/contracts/v1.md)、字段/错误码/事件清单、共享入口登记及迁移序号。不得在此实现设备导入或重连业务。

## 独占写入与步骤

拟新增 `src/{discovery,db/discovery,restful/discovery}/mod.rs` 骨架及 `frontend/src/types/discovery.ts`、`frontend/src/modules/discovery-api.ts`；路径均相对 easytier-web。实际公共AppState、迁移/路由挂载和页面接线由T19顺序完成。先复核基线和实际类型，再冻结owner/group、preview_revision、keep_service_ids、adb_service_id、endpoint_revision与withdraw协议；登记000019结构迁移和000020行为迁移，冲突时统一改名。列出密码保护的真实运行前提、服务类型识别及历史修正入口，不虚构现成设施。

## 完成条件与交接

契约标明拟新增与既存，覆盖所有用户确认分支；字段不能在各任务重复发明；共享文件单一写入。将冻结版本交T02–T18，并标清独立开发与联调依赖。文档定义完成不等于业务实现或服务可用。

## 后续验证

后续源码变更按授权核对公共DTO与调用一致性；本轮不运行类型检查、测试或构建。需求对应R01/R02/R03/R04/R06/R07/R08的入口、身份及结果条件。

## 本轮实际交付

公共契约、discovery Runtime 和共享路由已接线。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
