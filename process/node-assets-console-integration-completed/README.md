# 网络节点、设备与服务数据对接

当前分支：`feat/node-attributes-tech-console`。用户要求地图读取网络节点，服务管理读取设备内服务，并将设备管理提升为网络节点下方的一级菜单。

数据对接和菜单调整已完成，保留现有 3D 视图和资产权限。经用户授权，本轮前端构建通过，23 项逻辑测试、32 个不同的浏览器用例及 370 项后端测试通过；2 项既有大连接压力测试按原配置忽略。已复核桌面/手机截图和异步加载 100 台设备的镜头适配。

用户已授权推送并更新镜像，正在通过现有 GitHub Actions 发布 `linux/amd64` 镜像 `ghcr.io/cibederorg/easytier:feat-node-attributes-tech-console`。发布完成后回填提交、运行地址与镜像 digest。入口：[需求](require/README.md)、[计划](plan/README.md)、[执行记录](task/test-report.md)。
