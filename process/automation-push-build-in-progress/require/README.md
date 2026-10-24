# 需求

- 以用户指定的最新自动化中心分支交付。远端真实名称为 `feat/automation-center`，镜像标签为 `feat-automation-center`。
- 推送代码后自动执行已有前端、原生程序、镜像打包、启动检查及 GHCR 发布流程。
- 保留既有纯文档忽略、默认 amd64 平台及手动选择 arm64 的行为。
