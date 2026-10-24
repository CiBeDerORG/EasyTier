---
id: PLAN-ODS-09
title: 端口重连与单设备脚本退出
status: ready
requirements: [ODS-R07, ODS-R08]
tasks: [ODS-T15, ODS-T16, ODS-T17, ODS-T18]
---

# PLAN-ODS-09 端口重连与单设备脚本退出

## 目标与依据

落实[ODS-R07](../require/require-ods-07-reconnect-script-exit.md)和[ODS-R08](../require/require-ods-08-results-and-acceptance.md)：首次连接、变化后重连、原画面恢复、当前手机退出当前脚本、其他手机继续。当前触发器和执行器取消粒度为整批，媒体与前端尚无自动恢复；这些是已读代码缺口，不是验证结论。

## 小步骤

1. T01 冻结单设备退出、目标变更事件、清理证明及阶段结果契约；T02 提供基础服务身份/版本。事件必须来自已提交的有效目标变更，相同端口不得制造重连。
2. [T15](../task/task-ods-15-script-device-exit-api.md)实现后端按设备撤销、结果接收及新迁移，覆盖资产 IP/绑定更新、覆盖删除及 ADB 服务变化的触发器；T07 在这些行为交付后才开工。[T16](../task/task-ods-16-executor-device-exit.md)实现执行器按设备取消、旧目标清理和定向连接。T15/T16 按契约并行，不能用整批取消代替。
3. [T18](../task/task-ods-18-reconnect-ui.md)并行实现页面观看意图、阶段状态和新会话恢复。控件关闭、页面切换应取消恢复意图，清空旧动作队列。
4. T10、T11、T13、T15、T16 交付后，[T17](../task/task-ods-17-reconnect-coordinator.md)新增 Rust 协调模块，串起事件、单机退出、清理、新目标连接与可重试阶段。保存、公开应用、连接各自报告，不以映射失败阻塞可用组网目标的重连。
5. T19 接线公共入口和事件传递；T20/T21 在授权后完成接口、故障与真实手机验收。

## 并行和依赖

| 任务 | 开工依赖 | 仅联调依赖 | 独占范围 |
| --- | --- | --- | --- |
| ODS-T15 | ODS-T02 | ODS-T13、ODS-T16 | 两个自动化 `runs.rs`、登记的新事件/退出迁移 |
| ODS-T16 | ODS-T01 | ODS-T15 | 执行器 `runtime/worker/journal/adb` |
| ODS-T17 | ODS-T10、T11、T13、T15、T16 | 无 | 新增 `discovery/reconnect.rs`、`db/discovery/reconnect_store.rs`、`restful/discovery/reconnect.rs` |
| ODS-T18 | ODS-T01 | ODS-T14、ODS-T17 | `usePhoneSession.ts`、`PhoneCard.vue` |

依赖 T15 与 T16 的相互项仅阻塞联调，不形成开工循环。T17 串行是因为需要真实保存结果、旧目标退出和执行器清理接口，不能以占位成功替代。公共注册、协议类型、API 客户端交 T19；具体归属见各任务和[公共契约](contracts/v1.md)。

## 交付及验收

交付可追踪阶段状态、按设备清理证明及原观看者恢复链路。后续覆盖 ODS-R07-A01 至 ODS-R07-A06，特别是同批 A 改端口而 B 继续、重复发现不打断、乱序事件、未授权、断电后清理未知及映射失败而连接成功。不会自动重放脚本或恢复用户已关闭的画面。本轮不执行应用验证。
