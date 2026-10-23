# 官方结构 GHCR 镜像发布

**当前状态：双架构镜像已发布，等待包公开可见性设置；冷缓存匿名拉取被拒绝。**

## 目标

用户于 2026-10-23 授权将 `feat/node-attributes-main` 打包发布到 GitHub Container Registry，并要求沿用官方式镜像接口，使现有 deploy Docker 部署只需替换一个镜像地址。

- 目标镜像：`ghcr.io/cibederorg/easytier:feat-node-attributes-main`。
- 另发布 `sha-<完整提交号>` 固定版本标签及可锁定的 manifest digest。
- 平台：`linux/amd64`、`linux/arm64`，同一地址自动选择架构。
- 内容：`easytier-core`、`easytier-cli`、`easytier-web`、`easytier-web-embed`，后者含完整前端及 config generator。
- 接口：Alpine、`/sbin/tini`、`/usr/local/bin`、`/app`，默认入口继续为 `tini -- easytier-core`。
- 健康检查：读取 `EASYTIER_RPC_PORT`，未设置时用官方默认 15888；以进程级 `logger get` 支持尚无网络实例的管理模式。

## 进度

- [x] 检查当前分支与已通过的自动化回归。
- [x] 对照官方构建方式及 deploy 的 CLI、端口、挂载接口。
- [x] 实现双架构构建、容器冒烟和 GHCR 发布工作流。
- [x] 完成生产构建及两个原生架构的容器验证。
- [x] 发布镜像并回读 manifest、digest、两个架构。
- [x] 修复 Runner 的本地多架构 digest 冲突后重新验证认证拉取。
- [x] 执行两个全新 Runner 的匿名拉取验证，结果均为 unauthorized。
- [ ] 设置包为 Public 后重新确认匿名拉取，完成免登录单行替换条件。

## 入口

- 工作流：`.github/workflows/node-attributes-image.yml`。
- 冒烟检查：`.github/scripts/ghcr-smoke.sh`。
- 发布校验：`.github/scripts/ghcr-publish.sh`。
- 已发布镜像只读验证：`.github/workflows/node-attributes-image-verify.yml`。
- [轮次记录](task/test-report.md)。
- [完整构建与发布报告](../test/2026-10-23-ghcr-image-publish.md)。

本轮先制作并发布镜像。deploy 项目后续替换位置为 `easytier/deploy/docker-compose.yaml` 的 `image` 字段；本轮未修改 deploy 或运行用户的部署命令。OpenWrt 原生安装使用独立的二进制发布渠道，不属于 Docker 镜像地址替换范围。
