# 需求与交付汇总

|需求|状态|进度及问题|
|---|---|---|
|[节点详情、设备与服务](node-device-service-discussion/README.md)|已验证/待部署|T01–T15 完成开发与本轮授权验证，T16 待备份部署；[报告与截图](node-device-service-discussion/task/verification-2026-10-23.md)已归档。用户已授权提交/push到 `feat/node-attributes-main`；不追加测试，原运行服务未替换。|
|[设备卡片详情与导航](node-card-details-in-progress/README.md)|追加修改已完成、未验证|源码恢复卡片标识，将设备列表改称网络节点，并在仪表盘下增加空白网络拓扑入口；上轮13项用例通过，追加改动未验证或部署。|
|[main 网络节点属性](network-node-attributes-done/README.md)|已完成|原设备卡片新增地点/型号编辑，来源 IP 持久化；已部署当前云环境并截图。内置 GeoIP 仅 Country 精度，未现场接入用户实际 OpenWrt。|

每个需求使用 `<需求名>-<状态>/`，内部保留需求汇总及 `require/`、`plan/`、`task/`。用户管理、生态集成的历史文档保留在各自已有分支，没有移入本次 main 实施分支。
