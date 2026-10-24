# 自动化中心执行器设置

用户要求直接在自动化中心的设置页配置执行器。当前在 `feat/automation-center` 实现管理员设置、加密持久化、即时生效和手动连接检查。

- [需求与实现约定](require/requirements.md)
- [实施计划](plan/implementation.md)
- [验证与交付记录](task/test-report.md)

设置全局共用；普通账号不能读取或修改。环境变量继续作为首次配置的兼容来源，页面保存后以数据库为准。
代码和模拟验证已完成：Rust Web 405 项通过、2 项原有测试忽略；前端类型检查及构建通过；8 项浏览器回归、11 项执行器测试通过。已查看桌面/手机截图。这些本地结果对应第 1 轮实现与当时基线。

已推送至 `feat/automation-center`，合并远端最新 LuCI 接入检查和构建优化后的提交为 `72119aeec3b566e9761b32d5e57f553c8218562a`。[镜像构建 #26](https://github.com/CiBeDerORG/EasyTier/actions/runs/40084382069) 全部成功，耗时 13 分 45 秒，已发布 `ghcr.io/cibederorg/easytier:feat-automation-center`（`linux/amd64`）。已匿名核对远端 manifest、镜像配置、前后端提交号，以及分支标签和固定提交标签的摘要一致性，见[发布回执](task/artifacts/publication.json)。

未部署实机；独立手机执行器未构建镜像，特殊字符 Token 需要主服务与执行器同时更新。媒体执行器自身的依赖安装问题不在本次配置入口改动范围内。
