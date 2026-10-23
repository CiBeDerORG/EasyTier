---
feature: node-instance-binding
status: delivered
mode: debug
updated_at: 2026-10-23
---

# 节点设备角色与网络实例绑定：需求、计划和进度汇总

本轮基于 `feat/node-attributes-main` 已有节点/网关组/实体设备/服务管理，整理最新实例绑定与地址带出需求。上一轮需求、实现和验证保留在[原目录](../node-device-service-discussion/README.md)，本轮独立使用 `require/`、`plan/`、`task/`，不把旧任务的完成状态用于新能力。

## 当前进度

|项目|状态|问题与交付|
|---|---|---|
|用户决定归纳|已整理|实体关系、先角色后实例、网关原址代理/映射均支持、IPv4优先|
|前后端代码核对|只读核对完成|实例旧查询依赖在线/当前登录账号；资产没有绑定字段，存在两个新增入口|
|需求闭环|4份需求已完成|覆盖新建、旧资料补绑、实例变化、地址预览与保存、权限、删除及服务处理|
|工程方案|v2契约已冻结|T01已统一DDL、DTO、helper与组件协议。持久配置为登记依据，查询不改资料，异常保留快照并提供显式处理|
|计划与任务|6个计划、14项任务已拆分并开始实施|T01完成，T02–T12源码已接入；T13实际验证完成，T14当前云环境备份/部署完成|
|实现及验证|本轮完成|后端339通过，HTTP22通过，新页面24项检查通过；前端构建/原属性回归通过。11221已升级，原TCP客户端重连|

## 文档入口

- [需求范围与改动矩阵](require/require-01-scope-and-change-matrix.md)
- [后端绑定与地址需求](require/require-02-backend-binding-and-addresses.md)
- [前端流程与交互需求](require/require-03-frontend-workflows.md)
- [旧资料与生命周期闭环](require/require-04-lifecycle-and-compatibility.md)
- [开发计划汇总](plan/README.md)与[任务看板](task/README.md)
- [本轮冻结工程契约](plan/contracts/v2.md)与[逐轮记录](task/test-report.md)

## 本轮边界

资产主干为“网络节点→网关设备/直属实体设备；网关→下挂实体设备；实体设备→服务”。设备关联所在节点的网络实例；实例是绑定对象。普通子网代理和网段映射均为网关，无子网转发的实例用于直属普通设备。新建先角色后实例；下挂设备沿用网关实例，选择规则和主机部分生成地址。

保留现有登录/OIDC/config-server、网络实例操作、节点卡片入口、固定资产归属、数量口径和服务CRUD；不实现公网发布、网络拓扑画布、IPv6新推导、自动发现或定时任务。具体适配到原有前端页面，不另起一套控制台。

## 验证与交付

- [实际验证报告](task/verification-report.md)：构建、Rust、真实HTTP、浏览器、旧库升级和恢复演练；完整矩阵包含未覆盖项，不将测试通过等同用户实机网络可达。
- [交付记录](task/release-record.md)：11221预览已升级，TCP22030/原账号/节点属性/配置保留，客户端未重启且重连成功；已手工备份。
- [本轮截图](task/artifacts/2026-10-23/README.md)：独立合成数据，7张中文桌面/390px/弹窗图，2张显示修正后重拍。
- R01–R05原复审问题已修复并实测回填。新版新建改为先选角色和实例；旧IPv6资料保留，不自动绑定或改址。
- 2项既有压力测试、外部OIDC供应商及用户实机路由未执行；无新增公网/Dot入口，没有手动触发Actions/CI。纯文档更新不追加测试。

## 本轮文档分工

规划阶段由root、后端、前端与生命周期agent分别负责文档并交叉审阅。实施阶段按任务看板的唯一文件owner分工，root统一维护汇总、公共接线与交接状态。

## 2026-10-23 后续修正：实例候选显示主机名

- 用户确认保存实例配置后候选已出现；新增要求为候选卡片显示 host 名称，不能使用网络名称。
- 基线：`feat/node-attributes-main@4b7b63da0d18dd320fca933bf6705e24a5ec4012`；修改 `easytier-web/src/db/assets/instances.rs`，名称依次取非空实例 `hostname`、同账号同节点的 `devices.hostname`、实例 ID。普通和网关候选共用该实现。
- 状态：源码已修正并经人工交叉审阅；遵守调试模式，未运行测试、构建、类型检查、lint 或手动触发 CI，未部署。本节是后续修复记录，上述历史验证不覆盖此变更；详见[逐轮记录](task/test-report.md)。

## 2026-10-23 授权发布 hostname 修复镜像

- 用户明确要求“直接构建 提交到github镜像站”，本轮按该授权构建并发布到既有 GHCR 镜像。
- 修复源码提交：`e023e5945ba70ca7f0ef29279a92b7eb315a4948`。发布提交仅添加工作流发布记录注释和本轮文档，沿用已有工作流文件 push 触发规则。
- 目标：`ghcr.io/cibederorg/easytier:feat-node-attributes-main`，构建 `linux/amd64` 与 `linux/arm64`；具体 run、revision、digest 和成功状态将在实际结束后回填。
- 当前状态：发布请求准备完成，尚未取得本次构建或推送成功证据。详见[逐轮记录](task/test-report.md)。
