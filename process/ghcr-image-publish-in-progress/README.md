# 官方结构 GHCR 镜像发布

> 后续更新（2026-10-23）：当前分支 push 已改为只构建 amd64；镜像提速实现将页面作为镜像内独立资源，并按输入指纹复用原生程序。当前代码状态与未执行的验证范围见[镜像构建提速](../image-build-acceleration-in-progress/README.md)。下文保留首次双架构交付的历史记录。

**当前状态（2026-10-23 14:35，Asia/Shanghai）：镜像交付完成；amd64/arm64 均通过全新 Runner 的匿名实际拉取，可供 deploy 免登录替换镜像地址。**

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
- [x] 执行首次全新 Runner 匿名拉取，准确发现公开访问设置尚未生效。
- [x] 用户完成设置后重跑两个匿名 job，amd64/arm64 均真实下载成功，完成免登录单行替换条件。
- [x] 核对原源码 revision、默认入口、架构及总 digest，并更新最终发布报告。

最终验证：[运行 #39907312596，attempt 2](https://github.com/CiBeDerORG/EasyTier/actions/runs/39907312596/attempts/2)，整体 success。本次仅重跑两个匿名拉取 job，沿用原构建镜像；构建 revision 仍为 `9eb9e8aced76776c59668d8686affc2a8f302317`。

## 入口

- 工作流：`.github/workflows/node-attributes-image.yml`。
- 冒烟检查：`.github/scripts/ghcr-smoke.sh`。
- 发布校验：`.github/scripts/ghcr-publish.sh`。
- 已发布镜像只读验证：`.github/workflows/node-attributes-image-verify.yml`。
- [轮次记录](task/test-report.md)。
- [完整构建与发布报告](../test/2026-10-23-ghcr-image-publish.md)。

本轮先制作并发布镜像。deploy 项目后续替换位置为 `easytier/deploy/docker-compose.yaml` 的 `image` 字段；本轮未修改 deploy 或运行用户的部署命令。OpenWrt 原生安装使用独立的二进制发布渠道，不属于 Docker 镜像地址替换范围。
