---
id: ODS-R07
title: 即时重连、画面恢复与单机退出脚本
status: ready
baseline: dbbade81d8edfd51bc3844c393fb9afc650b51d1
---

# ODS-R07 即时重连、画面恢复与单机退出脚本

本需求已确认，实施尚未开始。以[ODS-R06](require-ods-06-automation-service-source.md)解析的当前手机 ADB 服务为目标，以[公共契约](../plan/contracts/v1.md)定义的事件、版本和阶段结果交接。

## 目标与规则

- **ODS-R07-01：触发。** 首次建立有效 ADB 服务尝试连接；同一服务的真实端口变化保存成功后，立即启动重连流程，不等待下次手动投屏。重复发现相同有效端口不重连，不打断已存在的画面和脚本。地址、服务替换等真实目标变化同样不能继续使用旧目标。
- **ODS-R07-02：单机退出。** 变化手机退出其当前参与的脚本控制，保留已有执行记录和已确认结果；未确认动作标为结果未知，不自动续跑、从头重跑或补发点击/输入。同一批次其他手机继续原任务，不能把单机退出升级为整批取消。
- **ODS-R07-03：先清理再使用新目标。** 停止旧目标的新动作，结束旧媒体、控件及本次设备占用后连接新端口。清理未确认时显示实际待清理状态，不抢占资源、不假报成功、不对执行器做全局 ADB 断开。
- **ODS-R07-04：恢复原画面。** 变更前正在观看、且原页面仍保留观看意图的手机自动恢复原画面；原先未投屏的仅连接 ADB，不主动开启画面。恢复使用新会话，旧输入队列清空；用户关闭画面或离开上下文后不得被迟到结果重新打开。
- **ODS-R07-05：真实状态。** 已有有效授权可复用；未配对、未授权、网络不可达或执行器不可用时，保存好的设备/服务保留，连接展示对应失败或待授权状态。配对仍由现有授权流程完成，不能把发现端口当作配对成功。
- **ODS-R07-06：重试与并发。** 仅重试失败阶段，不重放已完成的服务写入和脚本动作；重试前核对最新目标版本，迟到旧事件不得把新端口改回。数据库保存、公开映射应用和实际连接分别反馈，连接不等待公开映射成功才能使用有效组网目标。

## 典型场景

1. A、B 同批运行脚本，A 端口变化：A 退出当前脚本并重连，B 继续执行；A 未确认的最后动作不补发。
2. A 正在投屏，发现端口从 37123 变为 38901：清理旧会话、连接新端口，在原页面恢复新画面；相同端口再次发现没有中断。
3. 新发现手机未授权：ADB 服务保存成功，连接显示待授权；授权后仅重试连接阶段。
4. 连续出现两个新端口，较旧请求稍后返回：只承认最新有效目标；公开映射失败与连接成功分别展示。

## 验收标准

| 编号 | 可观察结果 |
| --- | --- |
| ODS-R07-A01 | 首次有效服务发起连接，真实端口变化及时启动重连，相同端口无额外中断。 |
| ODS-R07-A02 | 仅变化手机退出当前脚本；其他手机继续，历史结果完整且没有自动续跑/重放。 |
| ODS-R07-A03 | 原投屏恢复且无新旧资源重叠，未投屏者不自动开画面，手动关闭不被反转。 |
| ODS-R07-A04 | 未授权、连接失败、清理未确认分别可见；已保存资料不因连接失败回滚。 |
| ODS-R07-A05 | 重复/乱序事件、重试和页面切换不会恢复旧目标或发送历史动作。 |
| ODS-R07-A06 | 保存、公开应用、连接的成败分别可查，仅失败阶段可重试。 |

## 已核实的代码依据

[当前迁移](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/migrator/m20261024_000018_automation_center.rs) 中 `automation_connection_changed` 等触发器会置整批 `cancel_requested`；[runs.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/automation/runs.rs) 的 `authorize_run_step` 和 `apply_run_result` 仍基于整批取消确认清理。因此“其他手机继续”需要后续实施，不能沿用旧触发器宣称完成。

[执行器 runtime.py](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/automation-executor/automation_executor/runtime.py) 的 `renew` 拒绝变化 fingerprint，`cancel_run` 取消整个任务；[journal.py](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/automation-executor/automation_executor/journal.py) 的 `claim` 保留未清理占用；[usePhoneSession.ts](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/automation/usePhoneSession.ts) 断线后当前要求手动重开。上述机制可复用，但尚无本需求的自动恢复协调。

## 实施关联

[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)拆为后端单机退出、执行器清理、Rust 重连协调和页面恢复四项，分别由 ODS-T15 至 ODS-T18 实施；[ODS-R08](require-ods-08-results-and-acceptance.md)负责统一结果和验收。全部运行验收本轮未执行。
