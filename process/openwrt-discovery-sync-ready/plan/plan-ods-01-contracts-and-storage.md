# PLAN-ODS-01 公共契约与持久化基础

- 状态：计划已拆分；实现未开始。
- 需求：[R01](../require/require-ods-01-gateway-access.md)、[R02](../require/require-ods-02-device-discovery-review.md)、[R03](../require/require-ods-03-asset-apply-overwrite.md)、[R04](../require/require-ods-04-adb-discovery-service.md)、[R06](../require/require-ods-06-automation-service-source.md)、[R08](../require/require-ods-08-results-and-acceptance.md)。
- 唯一契约：[contracts/v1.md](contracts/v1.md)。

## 小步骤和任务

1. [ODS-T01 公共契约冻结与共享入口骨架](../task/task-ods-01-contract-and-shared-skeleton.md)：核对当前分支、现有符号和迁移序号，冻结归属、预览、唯一服务、端点事件、按设备退出与共享 DTO。
2. [ODS-T02 字段迁移与组内 IPv4 唯一](../task/task-ods-02-schema-and-scoped-identity.md)：扩展资料/凭据/服务类型及操作状态，迁移唯一性并保持外键和历史关系。

## 依赖与并行

T01 是统一开始点。T02 依赖 T01；LuCI 客户端、独立前端组件和执行器单设备退出可在 T01 后与 T02 并行。行为触发器改造只归 T15，不能挤进 T02 产生双重所有权。T07 在 T15 完成后开始，覆盖也继承该依赖。

## 所有权和交接

T01 维护公共契约/DTO及模块骨架；T02 拥有新结构迁移和相应 entity。业务模块与路由注册依[总所有权表](README.md)拆分，T19 后续顺序接线。输出字段/索引/约束说明、兼容状态、旧数据修正入口与未测清单，不能把原自动化端口自动转换成服务。

## 完成与后续验证

两个任务交付后，其他任务能引用唯一类型和持久化结构；空库/升级库约束、跨组同IP、服务唯一及关联保留交 T20 验证。源码、迁移和测试均尚未执行；本轮只完成计划。

