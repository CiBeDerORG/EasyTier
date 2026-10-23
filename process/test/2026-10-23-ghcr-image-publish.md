# EasyTier 官方结构镜像构建与 GHCR 发布

## 范围与授权

- 日期：2026-10-23，Asia/Shanghai。
- 仓库：`CiBeDerORG/EasyTier`。
- 分支：`feat/node-attributes-main`。
- 构建准备前提交：`3713e084746b45b5124f0746183e9ffdb07c4592`。
- 用户要求：打包发布镜像到 GitHub 的 Docker 仓库；按官方类似方式制作，使 deploy 项目后续只替换一个镜像地址即可使用。
- 目标地址：`ghcr.io/cibederorg/easytier:feat-node-attributes-main`；固定版本使用 `sha-<完整构建提交号>`，并记录最终 manifest digest。

## 方案

沿用仓库已有的官方式 `.github/workflows/Dockerfile`：以 Alpine 为基础，提供 tini、时区数据及 `/usr/local/bin` 下的四个程序，默认工作目录 `/app`，默认入口 `tini -- easytier-core`。

生产构建与官方 `core.yml` 对齐：先编译启用 `embed` 的 Web 并保存为 `easytier-web-embed`，再构建默认 workspace 的 core、CLI、普通 Web。前端生产包包含 config generator 及其 WASM 模块。

| 平台 | 原生 Runner | Rust 目标 | 附加 allocator feature |
| --- | --- | --- | --- |
| linux/amd64 | ubuntu-24.04 | x86_64-unknown-linux-musl | jemalloc |
| linux/arm64 | ubuntu-24.04-arm | aarch64-unknown-linux-musl | mimalloc |

这次发布上述两个平台。Dockerfile 保留原有其他平台的打包映射，但本轮没有构建或声称已验证 arm/v6、arm/v7、riscv64。

## deploy 接口兼容性

现有 Docker 部署通过挂载的脚本启动程序，使用 `/sbin/tini -g --`、`easytier-core` 和 `easytier-web-embed`。只要这些程序、参数和路径可用，Compose 的启动命令及数据挂载无需改变。

- RPC：7006；Web：7007；Config Server：7008 UDP。
- 数据路径：`/config`、`/cloud-managed-core`、`/data/et.db`。
- 替换位置：deploy 仓库 `easytier/deploy/docker-compose.yaml` 的 `image` 字段。
- 镜像健康检查原先固定 RPC 15888，并以 `node info` 要求至少一个运行中的网络实例。现在使用 `EASYTIER_RPC_PORT`（默认 15888）和进程级 `logger get`，可检查初次尚无网络实例的受管 Core。
- 本轮没有修改 deploy 项目，没有执行安装或重启用户服务。私有 deploy 脚本没有复制到此仓库，CI 使用独立的最小 CLI 接口 fixture。

## 计划验证与实际状态

| 项目 | 当前状态 |
| --- | --- |
| 前一轮 Web 自动化回归 | 已通过，仅代表 f24a9c7 的既有回归；[运行 #39900467503](https://github.com/CiBeDerORG/EasyTier/actions/runs/39900467503) |
| Bash 脚本语法及工作流 YAML 解析 | 已通过；尚未执行容器 |
| 完整生产前端及 config generator | 待执行 |
| amd64 四程序 release 构建与镜像冒烟 | 待执行 |
| arm64 四程序 release 构建与镜像冒烟 | 待执行 |
| GHCR 推送及双平台 manifest 验证 | 待执行 |
| 认证拉取两个平台 | 待执行 |
| 匿名拉取两个平台 | 待执行；尚不能承诺免登录使用 |

容器检查包含四程序加载、默认入口与零实例健康状态、现有 deploy 的 RPC/Web/数据库参数接口、Web 首页和 API metadata、config generator 页面及引用的 JS 资源。全部使用临时数据和 `--network none`，不访问真实设备或生产数据库。

两个原生架构检查通过后，将实际受检镜像保存为 tar 并由发布 job 原样加载推送；合并 manifest 后逐项比对平台及子镜像 digest，并回读固定 SHA 标签与分支标签的总 digest，最后实际拉取两个架构。

## 发布结果

尚未触发本轮构建。提交、运行链接、镜像 digest、实际检查及匿名访问结果将在执行后补充。不能把目标镜像地址当作已经发布成功的产物。
