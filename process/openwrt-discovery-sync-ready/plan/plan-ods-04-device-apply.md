---
id: PLAN-ODS-04
feature: openwrt-discovery-sync
title: 设备新增、更新与覆盖事务
status: ready
owner: null
requirements: [ODS-R03, ODS-R08]
tasks: [ODS-T07, ODS-T08]
---

# PLAN-ODS-04 设备新增、更新与覆盖事务

## 目标与依据

落实 [ODS-R03](../require/require-ods-03-asset-apply-overwrite.md) 的组内身份规则和选择性覆盖，并遵循 [ODS-R08](../require/require-ods-08-results-and-acceptance.md) 的分阶段反馈。基线 `dbbade81` 的 `db/assets/devices.rs` 已有事务、地址派生与幂等调用，但唯一范围仍是节点，删除会级联清理服务；本计划不把普通删除接口视为覆盖实现。

## 小任务、顺序与输出

| 任务 | 输入/依赖 | 交付 |
| --- | --- | --- |
| [ODS-T07 新增与更新](../task/task-ods-07-device-create-update.md) | T02 的组内约束及字段；T06 的预览；T15 的单设备失效行为 | 逐条校验并原子新增/更新、地址派生、幂等结果及提交后变更描述 |
| [ODS-T08 选择性覆盖](../task/task-ods-08-device-overwrite-services.md) | T07 的事务适配；T11 的公开关系迁移辅助 | 新设备 ID、按勾选迁移的原服务 ID、删除项清理、历史保留及旧目标失效描述 |

T07 等待 T02、T06、T15；T08 必须等待 T07、T11，并经 T07继承 T15 的触发器行为前提，不在真实状态迁移上强行并行。T07 与 [PLAN-ODS-07](plan-ods-07-publication-sync.md) 的 T11 可并行：前者只改设备模块，后者独占公开关系模块。T07 先复用既有 `sync_device_address` 保存预期配置，统一运行应用由 T11/T19 接线；T11、T17为T07联调前提，T17为T08联调前提，不等于新增实现硬依赖。

## 独占范围与协作

- T07 唯一修改既有 `db/assets/devices.rs` 和拟新增 `db/discovery/apply.rs`。
- T08 只新增 `db/discovery/replacement.rs`，调用契约化事务适配，不回改 T07/T11 文件。
- T02 拥有数据库迁移；T11 拥有公开关系迁移；T01/T19 维护公共声明、类型、路由和最终接线。接口签名与变更事件只引用[公共契约](contracts/v1.md)。

可让 [PLAN-ODS-05](plan-ods-05-device-sync-ui.md) 的 T09 同时按冻结契约制作预览和覆盖勾选界面，真实后端联调必须等 T07、T08。

## 交接与退出条件

向 T09 提供逐条成功/冲突/失败示例；向 T11/T17 提交带版本的公开目标变化及旧连接失效描述，网络副作用在提交后执行。完成标准对应 ODS-R03-A01～A08；不能删除执行历史，不能通过保留旧 ADB 配置把未保留的服务重新生成。T02只负责结构与唯一约束；整任务取消触发器的行为改造唯一归T15。T17确认旧会话退出前，新目标不能继续旧脚本。

ADB补采预览的成功/跳过结果携 `originating_operation_id`，由T19交T10续接原操作；T07不重新扫描或另建reconcile，过期由T10结束为待重新发现。

当前状态：两项任务均未认领、未实施。未来验证随实施授权开展，本轮不运行测试、构建或 CI。
