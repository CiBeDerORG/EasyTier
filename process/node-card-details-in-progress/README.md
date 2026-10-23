---
feature: node-card-details
status: in-progress
mode: debug
updated_at: 2026-10-22
---

# 设备卡片详情与按钮位置：需求/计划汇总

用户进入调试模式，要求保留旧截图中的 Hostname、Public IP、Network Count、Last Seen、Version、Machine ID 六项信息，并把新增“编辑属性”按钮移到原设置齿轮旁边。

追加调整：保留地区前的定位图标，版本恢复原版蓝底白字胶囊，网络数量恢复设置齿轮左侧的圆形徽标。上轮数量仍有展示，但位置和样式变成左侧“0 个实例”，未符合用户要求的原版呈现。本轮在同一张卡片内修正这三处，不改六项详情与原按钮行为。

追加命名与导航：将“设备列表”统一显示为“网络节点”，在仪表盘下方、网络节点上方加入“网络拓扑”菜单。只建立可访问的空白路由页，暂不实现拓扑页面内容；保留原 `deviceList` 路由和 API 标识，避免改变现有节点操作。现有 `MainPage.vue` 的工作区导航由桌面和手机菜单共用，新增项也自动参与标题和高亮计算。

基于 `feat/node-attributes-main` 当前实现：`DeviceList.vue` 卡片尚未展示 `DeviceDetails`，新增编辑按钮单独占一行；`DeviceDetails.vue` 已有相关数据，但在线状态不展示 Last Seen，Machine ID 位于折叠区。复用现有数据和详情组件，不新增 API、查询或状态任务。

|任务|负责人及独占文件|进度|
|---|---|---|
|卡片常显六项详情|前端 agent：`DeviceDetails.vue`，新增仅卡片启用的展示参数|已完成，专项浏览器检查通过|
|接入详情并移动按钮|root：`DeviceList.vue`，编辑属性与齿轮同排，操作栏位于详情上方|已完成，真实部署交互检查通过|
|调试记录|文档 agent 起草，root 收口：`task/test-report.md`|已记录本轮授权、结果与截图|
|恢复原卡片标识|root：`DeviceList.vue`，地区图标、版本胶囊、数量徽标|源码已完成；调试模式下未验证、未重新部署|
|列表命名与拓扑入口|root：`MainPage.vue`、`main.ts`、空白 `NetworkTopology.vue`；语言 agent：`cn.yaml`、`en.yaml`|源码已完成；调试模式下未验证或部署|

Public IP 沿用原连接地址（含协议和端口），与服务端自动记录的来源 IP 分开；Last Seen 使用已有 `last_seen`，缺失时回退 `report_time`。版本号与 Machine ID 完整展示。保留别名、删除、设置、地点/型号和表格/手机已有详情行为。

上一轮按调试模式未执行测试或构建。用户本轮明确授权“测试 我看看页面”，已执行本次卡片详情和按钮位置的针对性前端测试，并更新当前预览及截图；未运行后端全量测试或 Actions/CI，未提交或推送。此次授权不改变后续调试改动默认不自动测试的规则。

上轮前端类型检查与构建通过，13 项浏览器用例全部通过；真实部署核对六项信息、两个按钮位置与点击结果通过。当前云环境 `11221` 仍为该版本。追加的地区图标、版本胶囊、数量徽标、网络节点命名与拓扑菜单未构建、测试或部署，旧结果不代表这些改动。详见[测试报告](task/test-report.md)。

[卡片截图](/workspace/.easytier-node-attributes-preview/screenshots/card-details-closeup.png) · [完整页面截图](/workspace/.easytier-node-attributes-preview/screenshots/card-details-updated.png)
