# 网络节点、设备与服务数据对接

当前分支：`feat/node-attributes-tech-console`。用户要求地图读取网络节点，服务管理读取设备内服务，并将设备管理提升为网络节点下方的一级菜单。

数据对接和菜单调整已完成，保留现有 3D 视图和资产权限。经用户授权，本轮前端构建通过，23 项逻辑测试、32 个不同的浏览器用例及 370 项后端测试通过；2 项既有大连接压力测试按原配置忽略。已复核桌面/手机截图和异步加载 100 台设备的镜头适配。

已合入远端部署管理与自动化中心更新，推送源码提交 `f951e054c2b3ab3f27aa2ada2f72e2a584f14c9b`，并完成 `linux/amd64` 镜像发布：`ghcr.io/cibederorg/easytier:feat-node-attributes-tech-console`。[Actions 40025544463](https://github.com/CiBeDerORG/EasyTier/actions/runs/40025544463) 成功，独立匿名拉取确认分支标签和固定提交标签一致。

镜像 digest：`sha256:c034c14436d8b1fa9d3a093b3ccd646c7777be6c64a41c14ad7275b644ca46b1`。发布记录回填仅修改文档，不改变镜像源码版本。入口：[需求](require/README.md)、[计划](plan/README.md)、[执行记录](task/test-report.md)。
