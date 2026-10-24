---
id: ODS-T18
title: 重连状态与原投屏恢复
owner: null
status: todo
depends_on: [ODS-T01]
integration_dependencies: [ODS-T14, ODS-T17]
requirements: [ODS-R07, ODS-R08]
plan: PLAN-ODS-09
---

# ODS-T18 重连状态与原投屏恢复

## 目标、输入与输出

落实[ODS-R07](../require/require-ods-07-reconnect-script-exit.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)及[PLAN-ODS-09](../plan/plan-ods-09-reconnect-and-device-exit.md)。输入是 T01 的阶段契约、T14 主机上下文及 T17 最新目标状态；输出是原观看者画面恢复和真实连接状态。T14/T17 只阻塞联调，状态交互可按契约先实现。

## 独占拟改范围与依据

- `easytier-web/frontend/src/components/automation/usePhoneSession.ts`：`open`、`close`、`fail`、`stopLocal`、generation 与待发送队列。
- `easytier-web/frontend/src/components/automation/PhoneCard.vue`：卡片状态、观看意图、重试和恢复入口。

基线媒体失败会要求手动重开，旧会话未关闭禁止 `open`。不写 `AutomationCenter.vue`、`ConnectionDialog.vue` 或服务表单；协议类型、消息、客户端和事件接线统一提交 T19。

## 实施步骤

1. 记录用户是否正在观看及画质/卡片上下文；仅有效目标变化进入重连状态，相同端口刷新不重建媒体。
2. 停止本地旧画面和输入队列，等待旧资源清理及新 ADB 连接的真实结果。原页面仍希望观看时创建新会话和媒体，收到实际首帧后才显示画面已恢复。
3. 原来未投屏者不自动开画面；手动关闭、账号/设备切换及组件卸载使恢复意图失效，迟到响应必须关闭其产生的多余会话，不能覆盖新上下文。
4. 当前脚本退出仅展示状态与记录，不触发续跑、重新执行或旧点击补发。清理待确认、待授权、连接失败分别显示，重试只请求失败阶段；服务保存和公开应用的结果独立保留。

## 完成条件与交接

满足 ODS-R07-A03、A04、A05、A06：恢复前确认旧资源结束、恢复后有真实画面，其他卡片不中断。向 T14 交接卡片所需输入/事件，向 T19 交接共享调用和文案清单，向 T20 提供可复现状态场景。

## 后续验证

获授权后检查投屏中端口改变、未投屏连接、关闭/切页与恢复竞争、旧请求迟到、待清理重试、待授权、首帧超时和其他手机继续运行。使用契约样例不代表真实媒体已通。本轮只编写文档，未测试、构建或操作手机。
