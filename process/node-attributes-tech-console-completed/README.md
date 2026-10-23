# 节点属性与科技风控制台分支整合

用户要求将 `feat/node-attributes-main` 和 `feat/web-tech-console` 合并成新的最新分支。

- 整合分支：`feat/node-attributes-tech-console`。
- 节点属性来源：`origin/feat/node-attributes-main`，`9ed4d609c8e3da566ac06f7b04683a4575f76237`。
- 科技风来源：`feat/web-tech-console`，`c5843c2dd409f06dfcae76ca252e50ef2b5bf809`。
- 共同基点：`e307dadb4812fc42a43e4e7f210f11ffa980e02e`。
- 方式：保留双方历史的合并提交，源分支不变。完成本地整合后，按用户后续要求推送新分支并生成镜像。

已解决六处文件冲突，并完成前后端、浏览器和数据库升级验证。入口：[范围](require/README.md)、[整合计划](plan/README.md)、[本轮测试记录](task/test-report.md)。

新分支已推送，镜像构建与发布成功：[构建记录](https://github.com/CiBeDerORG/EasyTier/actions/runs/40015519054)。镜像为 `ghcr.io/cibederorg/easytier:feat-node-attributes-tech-console`，平台 `linux/amd64`，对应源码提交 `bc7922240df6840392bae3f9d8da1c9c69032932`。容器启动测试及独立匿名拉取验证通过，具体 digest 见测试记录。本次未部署线上服务。
