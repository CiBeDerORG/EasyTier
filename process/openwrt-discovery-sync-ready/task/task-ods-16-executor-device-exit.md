---
id: ODS-T16
title: 执行器单设备退出与旧目标清理
owner: null
status: todo
depends_on: [ODS-T01]
integration_dependencies: [ODS-T15]
requirements: [ODS-R07, ODS-R08]
plan: PLAN-ODS-09
---

# ODS-T16 执行器单设备退出与旧目标清理

## 目标、输入与输出

落实[ODS-R07](../require/require-ods-07-reconnect-script-exit.md)与[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)。输入为 T01 定义的目标版本、单机退出和清理契约；输出为可幂等调用的设备退出、旧目标清理及新端口连接能力。T15 仅阻塞联调，可按冻结协议并行实施。

## 独占拟改范围与依据

独占 `automation-executor/automation_executor/` 下 `runtime.py`、`journal.py`、`worker.py`、`adb.py`。关注 `Runtime._run_device/cancel_run/prepare/close_session`、`Journal.claim/release/recover`、`PreparedHelper.close`、`adb.prepare`。基线取消事件按整个运行共享，控件与端口指纹绑定，清理未知占用会保留。

不改 `app.py`、`validation.py`、媒体适配、Rust 或前端；路由与公共协议校验调整交 T19。清理媒体复用现有 `media.close`，不能绕过其未清理状态。

## 实施步骤

1. 实现公共契约的按设备 `withdraw` 内部动作：记录持久退出意图和独立取消信号，只停止指定设备，不取消同批父任务或其他设备；核对事件、执行器身份及旧指纹。保留整批取消原功能。
2. 结束该设备旧目标的输入、步骤进程和控件准备，复用媒体关闭；确认进程及占用释放后才回报清理成功。未知结果保留、不重发，重启不能复活旧工作。
3. 只针对旧设备的具体 ADB 地址清理连接，不执行全局断开或重启 ADB 服务。新端口连接沿用已有授权，未配对/未授权和网络失败返回真实原因。
4. 新目标可重新准备控件能力，但不创建或续跑脚本；接收重复退出/清理请求保持幂等，旧版本请求不得清理新会话。

## 完成条件与交接

输出包含目标、退出结果、清理确认与失败原因，供 T15 记录和 T17 协调；部分清理不能报告空闲。满足 ODS-R07-A02、A04、A05，其他手机仍运行且旧动作不重放。将新方法与路由校验需求交 T19，不由后续 T17 回头修改本任务文件。

## 后续验证

获授权后验证双设备运行中的单机退出、已发动作结果未知、媒体清理失败、重启后重复清理、新旧端口竞争及未授权连接。运行证明交 T20 汇总；本轮仅文档，未执行测试、构建或设备操作。
