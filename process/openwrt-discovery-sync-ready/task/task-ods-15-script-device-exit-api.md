---
id: ODS-T15
title: 后端按设备退出当前脚本
owner: null
status: todo
depends_on: [ODS-T02]
integration_dependencies: [ODS-T13, ODS-T16]
requirements: [ODS-R07, ODS-R08]
plan: PLAN-ODS-09
---

# ODS-T15 后端按设备退出当前脚本

## 目标、输入与输出

落实[ODS-R07](../require/require-ods-07-reconnect-script-exit.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)。输入为 T02 存储和[公共契约](../plan/contracts/v1.md)；输出为单设备撤销、事件记录、清理结果接收和批次状态汇总。T13/T16 只阻塞联调；T07 需先取得本任务的数据库行为再修改资产。

## 独占拟改范围与依据

- `easytier-web/src/db/automation/runs.rs`：`authorize_run_step`、`apply_run_result`、`aggregate`，以及拟新增单设备退出操作。
- `easytier-web/src/restful/automation/runs.rs`：`internal_authorize` 和拟新增单设备退出处理。
- 拟新增 `easytier-web/src/migrator/m20261024_000020_adb_service_events.rs`；T01 实施前核对最新编号并登记。替换旧触发器行为，不修改历史 `000018`。

不写 T13 的 `db/automation/mod.rs`、执行器及迁移注册；注册交 T19。基线触发器会置整批取消，结果接收也只接受整批取消的清理证明，需要一起调整。

## 实施步骤

1. 新迁移覆盖资产 IP/绑定变化、覆盖删除、ADB 服务删除/替换/端口变化：记录目标变更并撤销对应设备，不设置整批 `cancel_requested`。T02 仅提供结构，不承担这些行为。
2. 按运行、设备及目标版本持久化退出意图，拒绝该手机后续授权；同批其他设备仍获授权。原“取消整批”入口保持整批含义。
3. 接受 T16 单设备清理证明，保留已确认步骤和结果未知状态；迟到结果不得把已退出设备恢复运行。调整 `apply_run_result` 的清理校验和聚合，不要求整批取消才能确认单机退出。
4. 重复请求幂等；数据库事务只记录意图/事件，远端清理交 T17，历史执行记录不随覆盖删除。

## 完成条件与交接

满足 ODS-R07-A02、A05；任何资产地址更新/覆盖都不再误停同批其他手机。先向 T07/T10 交接触发器行为和事件，再向 T16/T17 交接退出接口及结果契约；共享注册需求交 T19。

## 后续验证

在授权后验证 A 改地址/端口或被覆盖而 B 继续、重复退出、迟到回调、结果未知及整批取消仍有效；迁移前后历史记录可读。当前仅文档，未运行迁移、测试、构建或 CI。
