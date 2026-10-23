---
feature: network-node-attributes
status: done
baseline: origin/main@e307dadb
updated_at: 2026-10-22
---

# 网络节点属性：需求/计划汇总

用户确认以仓库 main 为基线，保留原页面布局和操作，在单个 device 卡片增加“编辑属性”按钮，手填“地点、设备型号”；完成后在当前云环境部署并提供截图。本次将已讨论的来源 IP/所在地持久化后端移植到 main，手填地点独立于 IP 推测所在地，不加入备注或拓扑资源。

实施工作树 `/workspace/EasyTier-main-node-attributes`，分支 `feat/node-attributes-main`；fetch 后 main 为 `e307dadb`。此前用户管理与 IP 工作树保留，不合入用户管理功能、拓扑页面或前轮设备列表布局调整。

## 进度与入口

|阶段|入口|状态|
|---|---|---|
|需求|[require](require/README.md)|已确认并实现 main 上的两个手填字段与自动 IP 属性|
|计划|[plan](plan/README.md)|3 项计划完成；前后端并行实现后统一联调|
|任务|[task](task/README.md)|NAT-001、NAT-002、NAT-003 已完成|
|验收与部署|[verification](verification.md)|294 项后端检查通过、2 项原有用例跳过；2 项映射与10项浏览器场景通过；真实联调、部署及3张截图完成|

## 代码依据

- main 的 `db/entity/devices.rs` 以 `(user_id,machine_id)` 登记节点，无本次八个属性；`db/devices.rs::upsert_device_heartbeat` 更新上报信息，别名另写，适合让手填字段独立更新且不被注册覆盖。
- `restful/network.rs::handle_list_machines` / 别名更新路径已有登录用户、节点归属与中央管理边界；新增属性接口复用这些边界。
- `frontend/src/components/DeviceList.vue` 已有表格、卡片、手机列表、别名、删除、管理、搜索及刷新。单卡新增独立编辑入口，保留其现有事件、路由、刷新频率和视图切换。
- `client_manager/mod.rs`、Session 与 Storage 的 main 生产实现兼容前轮 IP 改动；只移植相关后端差异。用户管理特有测试身份结构不带入 main。
- `main.rs` 可通过 embed 构建直接运行 Web/API/config-server；部署使用本次构建、独立演示数据库与端口，不修改已有服务数据。

## 问题与决策

|编号|状态|处理|
|---|---|---|
|NAT-ISSUE-001|已确认|用户指定仓库 main；main 本身已有中央控制台布局，保持该布局|
|NAT-ISSUE-002|已确认|手填仅地点、设备型号，字段 `site_location` / `device_model`；自动 IP 和所在地只读|
|NAT-ISSUE-003|已解决|保存/取消/清空/重载/越权检查通过；真实部署重启、客户端重连后手填属性保留|
|NAT-ISSUE-004|已说明的限制|内置仅 Country 库；本次不自动下载城市库，不承诺省市精度|
|NAT-ISSUE-005|已完成|部署到当前云环境；截图来自真实后端和真实 TCP 客户端，地点/型号标注为演示值|
|NAT-ISSUE-006|已解决|移动端测试选择器匹配两个既有详情入口；收窄至单项直接子元素后复测通过，无产品代码变更|

当前云环境 Web/API 端口为 `11221`，config-server 为 `tcp:22030`，使用独立演示数据库。运行信息、账号文件位置、截图与复验步骤见验收记录。本次未提交或推送 Git；用户此前分支及其未提交工作保持原状。未新增定时任务或修改客户端 config-server 协议。
