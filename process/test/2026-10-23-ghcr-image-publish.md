# EasyTier 官方结构镜像构建与 GHCR 发布

**最终状态（2026-10-23 13:44，Asia/Shanghai）：双架构镜像已发布，内容和认证拉取通过；全新 Runner 的匿名拉取均被拒绝，等待包公开可见性设置。**

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

## 验证实际结果

| 项目 | 当前状态 |
| --- | --- |
| 前一轮 Web 自动化回归 | 已通过，仅代表 f24a9c7 的既有回归；[运行 #39900467503](https://github.com/CiBeDerORG/EasyTier/actions/runs/39900467503) |
| Bash 脚本语法及工作流 YAML 解析 | 已通过 |
| 完整生产前端及 config generator | 已通过；包含 WASM 编译与前端生产包 |
| amd64 四程序 release 构建与镜像冒烟 | 已通过；8 项 PASS，含 3 个 JS 资源检查 |
| arm64 四程序 release 构建与镜像冒烟 | 已通过；8 项 PASS，含 3 个 JS 资源检查 |
| GHCR 推送及双平台 manifest 验证 | 已通过；分支标签与固定 SHA 标签指向同一 index |
| 认证拉取两个平台 | 修复后均通过；同时验证实际平台、源码 revision 和默认入口 |
| 匿名访问 | 两个全新 Runner 分别拉取部署标签，均返回 unauthorized；免登录使用尚未达成 |

容器检查包含四程序加载、默认入口与零实例健康状态、现有 deploy 的 RPC/Web/数据库参数接口、Web 首页和 API metadata、config generator 页面及引用的 JS 资源。全部使用临时数据和 `--network none`，不访问真实设备或生产数据库。

两个原生架构检查通过后，实际受检镜像已保存为 tar 并由发布 job 原样加载推送；合并 manifest 后逐项比对平台及子镜像 digest，并回读固定 SHA 标签与分支标签的总 digest，最后按 index 中已核对的子镜像 digest 实际拉取两个架构。

## 发布结果与验证修复

- 镜像构建提交：`9eb9e8aced76776c59668d8686affc2a8f302317`。
- 构建与发布运行：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39903466090>，北京时间 2026-10-23 13:01:28 触发。
- 前端、amd64、arm64 三个构建 job 全部通过；每个平台 8 项容器检查通过。
- 已推送：`ghcr.io/cibederorg/easytier:feat-node-attributes-main` 与 `sha-9eb9e8aced76776c59668d8686affc2a8f302317`。
- 总 digest：`sha256:672bc12769b402a697870387b647d4c479efb26ae754cd75f84ca3c9559917aa`。
- amd64 子 digest：`sha256:2d978b8983487ccd46602ee453a29c8f036d04edcbbbe47a213412d5acb7c7b5`。
- arm64 子 digest：`sha256:576f82128b5ba52528ca6c57740204b0dfa108d01fec061d4a28850c79dfbecf`。

首轮发布 job 在发布后的第二个平台拉取时报 `cannot overwrite digest`，因此原运行整体为 failure，不能记为全绿。错误发生于 Docker classic image store：同一个总 index digest 的本地引用已关联 amd64 image ID，继续用同一引用拉 arm64 时拒绝覆盖。镜像推送、双平台 manifest 和两个标签的 digest 校验在该错误之前已成功。

修复认证及匿名两段拉取，改用已与 index 核对的各平台子 digest，并检查拉回镜像的实际架构、源码 revision 和默认入口。新增只读验证工作流，下载首轮 publication artifact 中的既有 digest 证据，保留原制品 revision `9eb9e8a`；不重编译、不修改镜像或重推标签。匿名验证同时读取固定 digest 的总 manifest 并核对平台与子镜像映射，然后实际拉取两个子镜像。

依据：[Docker 官方 GitHub Actions 多平台说明](https://docs.docker.com/build/ci/github-actions/multi-platform/)；[Moby 官方同类问题 #43188](https://github.com/moby/moby/issues/43188)。

## 构建证据

- [前端产物](https://github.com/CiBeDerORG/EasyTier/actions/runs/39903466090/artifacts/12479812880)。
- [amd64 构建与容器日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/39903466090/artifacts/12480423073)。
- [arm64 构建与容器日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/39903466090/artifacts/12480627831)。
- [首轮发布和 manifest 证据](https://github.com/CiBeDerORG/EasyTier/actions/runs/39903466090/artifacts/12480233285)。

## 只读重验证结果

- 修复提交：`ce3d8fde1fbf9fcdbf765f5d84e186805ca2f6f2`。
- [只读重验证 #39906421259](https://github.com/CiBeDerORG/EasyTier/actions/runs/39906421259) 已完成，job 为 success。
- amd64 与 arm64 子镜像实际认证拉取成功；平台、revision `9eb9e8a`、官方入口均通过。
- 总 index 与两个标签继续匹配原发布 digest。
- publication.json 明确记录 `anonymous_pull_verified: false`。匿名总 manifest 返回 `unauthorized`。后续 child pull 使用了同一 daemon 的已有镜像缓存，输出 Image is up to date，不能据此判定匿名访问成功。
- [重验证证据](https://github.com/CiBeDerORG/EasyTier/actions/runs/39906421259/artifacts/12481225860)。
- 已补充两个全新 Runner 的匿名拉取，使用空 Docker 配置、无登录步骤，直接验证分支标签的实际拉取权限。

## 冷缓存匿名拉取结果

- 验证提交：`f6669769e28f2ebb9cc314842820ad1db36b0dbd`。此提交只修改验证工作流及报告，制品版本仍为 `9eb9e8a`。
- [运行 #39907312596](https://github.com/CiBeDerORG/EasyTier/actions/runs/39907312596)，北京时间 2026-10-23 13:43:42 完成，整体 failure。
- 认证和 manifest 重验证 job：success；原总 digest、源码 revision、入口和两个平台仍一致。
- amd64 匿名 job：failure；无镜像缓存、空 Docker 配置、未登录，在部署标签 HEAD 请求处返回 `unauthorized`。
- arm64 匿名 job：failure；同样在部署标签 HEAD 请求处返回 `unauthorized`。
- 两个 job 均设置 `permissions: {}`，没有 checkout、镜像导入、认证拉取或 registry 登录步骤，各自使用独立的新 Runner。
- [amd64 匿名失败日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/39907312596/artifacts/12481650879)。
- [arm64 匿名失败日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/39907312596/artifacts/12481411853)。
- [同轮认证与 manifest 验证证据](https://github.com/CiBeDerORG/EasyTier/actions/runs/39907312596/artifacts/12481347428)。

这两个失败准确暴露尚未完成的匿名访问条件；没有降低检查标准或将其记为通过。公开可见性设置后，只需重跑这些匿名 job，不需要重新编译、打包或推送镜像。

## 可用性剩余事项

镜像内容和认证拉取已可用，但“deploy 只替换地址、无需登录”的条件仍被匿名访问拒绝阻塞。GHCR 首次发布默认 private；关联公开源仓库不等于包公开。公开可见性应在组织 Packages → easytier → Package settings → Danger Zone → Change visibility → Public 设置。

当前 GitHub 连接器没有包可见性修改能力；Browser 返回缺少发起会话的浏览器上下文，无法进入包设置。尚未改动包的可见性，也未修改 deploy 或重启任何部署。需要由有包管理权限的用户设置 Public，或提供可用的已登录浏览器连接后继续。设置完成后可复用现有镜像重新执行匿名验证，无需重编译。

依据：[GitHub 包访问权限与可见性](https://docs.github.com/en/packages/learn-github-packages/configuring-a-packages-access-control-and-visibility)。

## deploy 的单行替换

在 `CiBeDerORG/deploy` 仓库中的 `easytier/deploy/docker-compose.yaml`，将 image 字段改为：

```yaml
image: ghcr.io/cibederorg/easytier:feat-node-attributes-main
```

匿名访问确认后，在 deploy 仓库根目录执行 `sudo bash deploy.sh easytier update`。需要修改仓库里的 Compose；仅改安装目录副本会被更新流程同步覆盖。现有 cloud 和 intranet 的入口、端口、数据挂载可沿用。OpenWrt 原生安装及未包含在本轮的 CPU 架构不受本 Docker 镜像更新。


## 固定版本与更新方式

日常使用分支标签：

```text
ghcr.io/cibederorg/easytier:feat-node-attributes-main
```

固定本次源码版本的标签：

```text
ghcr.io/cibederorg/easytier:sha-9eb9e8aced76776c59668d8686affc2a8f302317
```

按内容锁定本次双平台镜像：

```text
ghcr.io/cibederorg/easytier@sha256:672bc12769b402a697870387b647d4c479efb26ae754cd75f84ca3c9559917aa
```

分支标签会在后续显式发布时更新；固定 SHA 标签和 digest 用于版本追踪或回退。当前工作流只在对应工作流文件变化或显式手动触发时运行，普通源码和报告提交不自动发布。最终报告提交仅修改文档，不触发构建。
