---
feature: easytier-ecosystem-integration
status: in-progress
baseline: feat/web-user-management@1e92296b17fc2e85dcd4521b6012232e11abed95
updated_at: 2026-10-21
---

# 网络拓扑与资源集成需求/计划汇总

本文件先汇总范围、现状、进度和问题，后续需求、计划及任务从这里展开。详细内容必须引用已核对代码或明确标为拟新增/待核实，不能把目标模型写成现有功能。

本轮按“需求 → 小计划 → 小任务”继续拆分：保留11个原工作包，安排15项独立任务（ECO-101～115），先核实源码/API与页面证据，再交付契约和公共基础，随后六组独立页面/适配任务并行，最后串行组合、接线、真实联调与验收。当前已授权实施，首批基线任务核查中；未知外部能力仍列待核实，不据此补造接口。

## 目标与基线

目标是在 EasyTier Web 按“网络拓扑 → 网络节点 → 类型 → 服务/设备组 → 设备”组织资源，补充地点、型号、访问及导航入口。本期按已存在的接口能力进行页面设计与接入；缺口记录为未支持/待确认，不自动扩展为资源后端、网关或外部集成开发。deploy 安装方案后置。

2026-10-21 只读核对的实现基线为 `feat/web-user-management@1e92296b17fc2e85dcd4521b6012232e11abed95`，对应实现工作树干净。采用文档快照为 `docs/architecture-and-user-management-plan@670eb3b42800d42333356da0cb15f41d76c92f1e`。没有将用户管理实现合并到文档分支；设计启动时应核实实际采用提交及其后续差异。

## 进度与内容入口

| 内容 | 文档进度 | 实现进度 | 下一步 |
| --- | --- | --- | --- |
| [需求](require/README.md) | 模型、范围和用户约束已形成草案；外部能力仍待核实 | 尚未实施本需求页面或外部集成 | 根据 ECO-001 的实际页面/API 清单细化，不把建议字段当作现有 DTO |
| [PLAN-ECO](plan/README.md) | 11个工作包各有 `plan-eco-编号-名称.md`；另有一份公共说明，不计工作包 | 工作包未执行，不计作已认领任务或任务完成率 | 先明确基线与可用接口，再固定接入清单和页面契约 |
| [任务看板](task/README.md) | 已生成15份 task-eco-101～115 明细，覆盖11个计划工作包 | ECO-101/102 done、ECO-103 in_progress，其余12项backlog；已完成2/15。 | 先安排101/102核查，随后103契约、104/105基础、六组独立模块，再组合/接线/联调 |

总体为 `in-progress`；页面取证按任务执行，未取得证据前不作通过结论。

## 当前事实与代码依据

以下链接固定到已核查的 `1e92296b`，不是引用当前文档分支的旧源码。

| 编号 | 类型 | 现状/目标 | 代码依据与处理 |
| --- | --- | --- | --- |
| ECO-EV-001 | 事实 | 宿主已有登记信息，但不是手机/家居设备模型 | [devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-web/src/db/entity/devices.rs) 的 `Model` 使用 `user_id`、`machine_id` 并存 hostname、alias、OS 等；此模型未含人工地点和型号字段，不能编造已有保存能力 |
| ECO-EV-002 | 事实 | 中央网络成员记录引用宿主，已有机器查询入口 | [network_members.rs](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-web/src/db/entity/network_members.rs) 的 `Model.device_id`；[network.rs](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-web/src/restful/network.rs) 的 `handle_list_machines` 与 `/api/v1/machines`，具体页面字段和权限仍须逐项对应 |
| ECO-EV-003 | 事实 | 已有 TCP/UDP 端口转发配置原语 | [gateway.rs](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-core/src/config/gateway.rs) 的 `PortForwardConfig` 含绑定地址、目标地址和协议；它本身不证明资源级公网授权、导航发布或撤销活动流能力已具备 |
| ECO-EV-004 | 事实 | 既有 Web 含周期请求，复用需控制副作用 | [MainPage.vue](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-web/frontend/src/components/MainPage.vue) 的 `setInterval(loadNavigation, 10_000)`；[NetworkDetail.vue](https://github.com/CiBeDerORG/EasyTier/blob/1e92296b17fc2e85dcd4521b6012232e11abed95/easytier-web/frontend/src/components/NetworkDetail.vue) 的 `setInterval(loadNodeDetail, 5_000)`，新增页面不得借用这些定时器扩展资源状态查询 |
| ECO-EV-005 | 拟新增/待核实 | 节点资料扩展、服务/设备组/设备层级、公网资源发布与 Navloom 入口 | 来自用户目标及[需求](require/README.md)，尚未完成对应接口能力核对；不能推定 EasyADB、家居、Navloom 或 deploy 仓库/API 已接入 |

## 问题、缺口与下一步

| 编号 | 问题与来源 | 当前状态 | 影响 | 下一步/解除条件 | 负责人 |
| --- | --- | --- | --- | --- | --- |
| ECO-ISSUE-001 | 地点/型号及业务资源所需字段、写入与权限能力，见上述模型与[计划待决项](plan/README.md#8-当前待决项与记录) | 待核实；已知 devices 模型不含部分目标字段 | 不能承诺资料可保存或资源可控制 | ECO-101/103 逐项列出已有、缺失、未验证的实际 API 与字段，缺口不自动转后端任务 | `null` |
| ECO-ISSUE-002 | EasyADB、家居、Navloom 的实际仓库/API、端点与权限未核实，见[需求待细化项](require/README.md#13-待细化事项与验证记录) | 待核实 | 外部资源操作与导航发布的可接入范围未确定 | 实际涉及接入前核对来源与契约；未核实能力明确显示未支持 | `null` |
| ECO-ISSUE-003 | 已有组件包含周期状态行为，见上述 MainPage/NetworkDetail 代码 | 已确认的复用约束 | 直接复用可能违反新增页面不增加定时查询的约定 | ECO-102记录旧请求，103明确纯展示/按需边界，113正式接线不得新增轮询、心跳、订阅或定时任务 | `null` |
| ECO-ISSUE-004 | 页面证据、隔离账号/设备、公网入口及验收环境，见[计划待决项](plan/README.md#8-当前待决项与记录) | 待核实；本次未启动应用 | 对应真实行为不能判为可用或通过 | 在实际设计/实施时准备相应证据；环境缺失只阻塞对应验收，记录范围 | `null` |

状态以可观察代码和记录为限；尚未全面核查的项目不写成“无问题”。保留 `config-server` 客户端连接、认证、配置下发及接入命令；新增页面仅按进入页面、切换对象、手动刷新或明确操作请求按需查询。

## 关联需求

- [需求](require/README.md)
- [计划](plan/README.md)
- [任务看板](task/README.md)

以 EasyTier Web 为入口，按网络拓扑、节点、服务/设备组及设备组织资源，按已有接口能力安排访问与导航。当前已有需求、11份小计划及15项任务文档，尚未实施页面或外部集成。

复用[已完成用户管理](../web-user-management-done/README.md)。[现有 Web 交互优化与页面美化](../web-ui-optimization-in-progress/README.md)已独立管理，不属于本需求工作包，也不是拓扑接入的开发前置。双方涉及共享宿主文件时，按[全局规则](../README.md)串行交接。

## 维护记录

2026-10-21：根据交付基线只读核查设备、成员模型、机器 API、端口转发配置及现有请求入口，补齐汇总和待核实项。未编造外部 API 或后端能力，未认领/实施工作包，未运行构建、测试或 CI。

2026-10-21：将计划拆成索引、公共说明及11个独立工作包文件；任务入口链接具体计划，当时任务明细待依据实际契约拆分，未将文件整理计作实施进度。

2026-10-21：按最新“需求拆分”定义完成15项小任务，实际状态全部backlog，待核实事项落到101/102/103等任务与启动条件；计划/看板/明细同步多Agent批次、单一写入者和跨需求交接，未执行应用检查。


## 当前实施快照

ECO-101/102 done、ECO-103 in_progress，其余12项backlog；已完成2/15。 首批基线已由监工审查，ECO页面取证已验收，真实设备/公网仍未验证。当前基线提交 `af28b81e`，代码基础 `1e92296b`，指定文档 `670eb3b4`。用户授权3执行+1监工：exec1负责生态契约后续，exec2负责ECO-102，exec3负责WUI-102，iteration_monitor负责独立关卡审查。状态事实源为任务明细，未运行场景不作通过结论。
