# GHCR 镜像轮次记录

## 2026-10-23 发布准备

- 用户明确授权构建并发布 GHCR 镜像，要求保持官方镜像接口，并适配现有 deploy 的单行镜像地址替换。
- 源码基线：`3713e084746b45b5124f0746183e9ffdb07c4592`，包含已通过 Web 回归的 `f24a9c7aad2ce8233045e8a1b43800ddf5999dd8`。
- 新增工作流仅在当前分支的该工作流文件变化或手动调用时执行；普通源码或报告提交不自动发布。
- 构建采用官方的 musl 目标与四程序打包顺序；前端和 config generator 一并内嵌。
- Dockerfile 的运行目录、默认入口及程序路径保持官方接口；健康检查适配现有 RPC 端口及零网络实例状态。
- 发布 job 仅在两个架构的容器检查全部通过后执行，上传的 tar 来自实际受检镜像；仅发布 job 获得 `packages: write`。
- 容器检查使用独立编写的 CLI 接口 fixture、临时数据和 `--network none`，不复制或执行私有 deploy 脚本。
- 当前生产构建、容器执行、GHCR 推送和匿名拉取均未完成，不宣称镜像已可用。实际运行和结果以[完整报告](../../test/2026-10-23-ghcr-image-publish.md)为准。
- 提交前已执行两个 Bash 脚本的语法检查和工作流 YAML 解析，均通过；已独立复核构建顺序、发布权限与受检镜像 digest 链路。

## 2026-10-23 双架构构建完成与发布验证修复

- 运行 #39903466090：前端、amd64、arm64 构建及镜像冒烟均通过，各架构 8 项 PASS。
- 两个平台镜像已推送，分支标签与固定 SHA 标签的总 digest 均为 `sha256:672bc12769b402a697870387b647d4c479efb26ae754cd75f84ca3c9559917aa`。
- 原发布 job 的认证拉取第二个平台报 `cannot overwrite digest`；这是 Runner Docker classic store 对同一个 index 引用的本地映射限制。原运行整体为 failure，如实保留。
- 修复认证与匿名拉取均使用已经核对到总 index 的各平台子 digest；补充实际架构、源码 revision、入口断言。
- 新增只读镜像验证工作流，仅有 contents/actions/packages read 权限，复用原运行的 publication artifact；制品源码仍为 `9eb9e8a`，不把验证提交标成制品版本。
- 修改后的 Bash 语法和两个工作流 YAML 解析均通过。重验证及匿名访问仍待实际运行。

## 2026-10-23 认证拉取通过，补充冷缓存匿名验证

- 修复提交 `ce3d8fde1fbf9fcdbf765f5d84e186805ca2f6f2` 的只读验证运行 [#39906421259](https://github.com/CiBeDerORG/EasyTier/actions/runs/39906421259) 已完成，job success。
- 两个平台按对应子 digest 实际认证拉取成功；平台、源码 revision、默认入口、index 与标签均一致。
- 匿名读取总 manifest 返回 unauthorized；同 daemon 已有子镜像缓存的 pull 显示 Image is up to date，不能判定匿名可用。
- 新增两个独立 Runner 的匿名拉取 job，permissions 为 {}，不 checkout/login/import，不共享认证镜像缓存；直接拉部署标签，失败会使 job 失败。
- 新 job 核对实际平台、原制品 revision、总 index digest 和默认入口，所有验证证据名称按 run_attempt 区分，可在可见性更新后单独重跑。
- 修改后的 YAML、3 个 Bash 块和 1 个 Python 块静态检查通过，实际匿名拉取待运行。
- GitHub 连接器无包可见性修改能力；Browser 连接返回缺少发起会话的浏览器上下文，因此当前未能进入包设置，尚未修改可见性。
