---
id: ODS-T17
title: 服务端口事件与重连协调
owner: Codex
status: done
depends_on: [ODS-T10, ODS-T11, ODS-T13, ODS-T15, ODS-T16]
integration_dependencies: []
requirements: [ODS-R07, ODS-R08]
plan: PLAN-ODS-09
---

> 本轮实施更新（2026-10-24）：用户已授权代码开发和模拟联调，Codex 顺序执行共享文件修改。下文的“本轮仅文档/待测试授权”等措辞是拆分阶段的历史边界；当前结果以[任务看板](README.md)与[验证记录](test-report.md)为准。实机及部署仍未执行。

# ODS-T17 服务端口事件与重连协调

## 目标、输入与输出

落实[ODS-R07](../require/require-ods-07-reconnect-script-exit.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)。输入为服务已保存事件、T11 公开应用结果、T13 权威目标、T15 单机退出和 T16 清理/连接输出；这些真实依赖完成后再串联。输出为可追踪、可阶段重试的单设备重连结果。

## 独占拟新增范围

- `easytier-web/src/discovery/reconnect.rs`：协调状态转换。
- `easytier-web/src/db/discovery/reconnect_store.rs`：使用 T01 登记、T02/T15 交付的持久化结构记录阶段与版本。
- `easytier-web/src/restful/discovery/reconnect.rs`：状态查询/失败阶段重试处理，路由名称遵循[公共契约](../plan/contracts/v1.md)。

以上均为拟新增文件。不开第二套服务表，不改执行器、T13/T15 文件及已有会话入口；模块/路由注册、事件触发接线由 T19 完成。基线现有 `sessions::close_remote` 和 Runner 仅提供可复用调用，不构成自动协调。

## 实施步骤

1. 消费已提交事件，首次有效服务连接；比较 `adb_service_id`、`endpoint_revision` 和有效目标，相同端口无变化则结束，不打断运行。
2. 立即记录重连意图，先令该手机退出当前脚本，再请求清理旧目标；确认清理后连接最新服务端口，绝不自动创建/续跑脚本。
3. 同设备按版本协调；每次远端调用前后复核目标，乱序和重复事件不得覆盖新状态。未授权、执行器离线或清理未知按实际阶段保留。
4. 保存、公开应用和连接分开报告。T11 应用失败不回滚已保存服务，也不阻止有效组网目标连接；重试只调用未完成阶段。
5. 为原观看者提供可恢复的新目标状态，画面新会话由 T18 建立，不在服务端凭空开启无人观看的媒体。

## 完成条件与交接

满足 ODS-R07-A01、A04、A05、A06；输出可定位的阶段结果和重试入口。向 T12/T18 交接状态，向 T19 交接入口、事务提交后事件触发和共享调用清单；不得要求 T17 接管 T16 的实现文件。

## 后续验证

授权后覆盖未变端口、首次连接、连续端口变化、保存后中断、映射失败但连接成功、未授权及重复重试。阶段成功必须来自真实结果，不能由数据库写入推断连接成功。本轮未运行测试、构建或 CI。

## 本轮实际交付

端点事件触发一次清理/连接；失败保留阶段、显式重试；前驱清理与旧目标多指纹记录已验证。 统一证据见[验证记录](test-report.md)，使用/升级见[交付说明](delivery-record.md)。
