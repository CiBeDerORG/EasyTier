# 需求与交付汇总

|需求|状态|进度及问题|
|---|---|---|
|[自动化中心 push 自动构建](automation-push-build-in-progress/README.md)|平台已恢复，构建跟进中|用户已重新启用 Actions，后续代码 push 不默认跳过 CI；本轮通过工作流提交启动最新分支的 amd64 镜像构建。|
|[全局完整性审计](global-integrity-audit-in-progress/README.md)|发现未闭环，修复待跟进|包含地图合并后的当前分支、关联 deploy Agent 和手机执行器；测试通过与实际缺口分开记录，见[报告](global-integrity-audit-in-progress/task/test-report.md)。用户已确认手动服务导入为必做范围。|
|[OpenWrt发现同步与手机ADB联动](openwrt-discovery-sync-in-progress/README.md)|代码和模拟验证通过|feat/automation-center；普通发现/覆盖、唯一ADB服务、公开目标同步、单机退出和画面恢复已实现；[验证记录](openwrt-discovery-sync-in-progress/task/test-report.md)、[升级回退](openwrt-discovery-sync-in-progress/task/delivery-record.md)。实机验收另行安排，未发布部署。|
|[自动化中心](automation-center-in-progress/README.md)|模拟验证通过，待实机|feat/automation-center 已包含技术控制台页面优化、手机墙与悬浮控制、有限脚本；本轮通过 OpenWrt 联动接入唯一 ADB 服务与单设备退出，模拟证据见上项；真实媒体和实机仍待验收。|
|[用户管理与删除停网](user-management-in-progress/README.md)|已实现，待验证|管理员菜单、全量用户列表和逻辑删除；禁用全部网络配置，保留节点/设备/服务，在线停止与离线未确认分开反馈。本轮按用户指令先提交推送，测试与部署未执行。|
|[节点设备角色与网络实例绑定](node-instance-binding-planned/README.md)|已验证并部署当前云环境|14项任务完成，角色/实例绑定、IPv4派生、旧资料兼容与配置变化处理已交付；[验证报告](node-instance-binding-planned/task/verification-report.md)、[备份部署记录](node-instance-binding-planned/task/release-record.md)。无公网/Dot访问入口。|
|[节点详情、设备与服务](node-device-service-discussion/README.md)|已交付|原16项任务及R01–R05复审修复已随实例绑定版验证部署；原报告保留历史证据，最终结果见新轮报告。|
|[设备卡片详情与导航](node-card-details-in-progress/README.md)|已随新版验证部署|原节点属性13项浏览器回归通过，网络节点/空白网络拓扑入口随新制品交付；目录保留历史记录。|
|[main 网络节点属性](network-node-attributes-done/README.md)|已完成|原设备卡片新增地点/型号编辑，来源 IP 持久化；已部署当前云环境并截图。内置 GeoIP 仅 Country 精度，未现场接入用户实际 OpenWrt。|

每个需求使用 `<需求名>-<状态>/`，内部保留需求汇总及 `require/`、`plan/`、`task/`。用户管理、生态集成的历史文档保留在各自已有分支；本轮用户管理单独记录，不覆盖历史交付结论。

