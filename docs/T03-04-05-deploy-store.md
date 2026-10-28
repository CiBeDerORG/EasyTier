# T03-04 / T03-05 — EasyTier 中央 Deploy 商店本地工作副本与只读目录

> 2026-10-28；EasyTier 分支 `feat/automation-center`。
> 当前状态：**代码已提交，GitHub Actions 最新构建待核验；真实私有 Deploy 仓库带只读凭证的运行验收尚未完成。**
> 本轮未实现安装、接管、部署包分发、自动同步和后台定时拉取。

## 复用架构与入口

- 不新建 Agent 或主机资产体系；商店是 EasyTier **控制端全局目录**，不依赖某台设备在线。
- 既有网页登录会话在 REST 层验证权限；已有 admin 组才可执行手动 Git 刷新；普通登录用户只可浏览目录。
- 后端源代码：`easytier-web/src/deploy_catalog.rs` 和 `easytier-web/src/restful/deploy_catalog.rs`；新数据库迁移 `m20261028_000044_deploy_catalog.rs`，维护 `deploy_catalog_state` + `deploy_catalog_apps`。
- 前端入口：工作空间侧边栏 **应用商店**，地址 `/#/h/store`，不替换历史的“应用与服务”模块；文件 `DeployStore.vue`。
- 已读取的数据库快照和对应 `manifest_sha256` 在 GET 时均不查询 Git/Docker/Agent，不因用户搜索、切换页面或查看详情触发网络。

## 配置与目录

在运行 EasyTier **控制端**的容器/原生进程提供：

```env
# GitHub 只读令牌；需要能够读取 CiBeDerORG/deploy 仓库。
# 不要把实际值提交到 Git、粘贴到网页或放入 URL。
ET_DEPLOY_GIT_TOKEN=<YOUR_READ_ONLY_GITHUB_TOKEN>

# 在 Docker 部署中映射为持久卷目录；不可放入临时容器层。
ET_DEPLOY_CATALOG_DIR=/var/lib/easytier/deploy-catalog
```

- 源仓库在控制端**固定**为 `https://github.com/CiBeDerORG/deploy.git`，只拉 `main`，不接受浏览器传任意 Git URL/分支，避免 URL 注入和 SSRF。
- Git 认证走 **child-process 环境变量里的 scoped HTTP extraheader**；Token 不放进命令行、clone URL、数据库、JSON 或返回错误。执行超时/失败显示统一异常，不回传 Git stderr 凭证。
- 镜像 `.github/workflows/Dockerfile.node-attributes` 增加 `git` 与 `ca-certificates`，否则旧 GHCR 镜像手动刷新必定失败。
- 默认本地目录 `.deploy-catalog` 适合原生短期调试；生产 Docker **必须显式指定持久位置和卷映射**，不然重建镜像后可能丢失本地 Git 工作副本，即使 SQLite 保留了上次索引。
- 每次操作只在后台管理员点击“手动刷新 Git”后执行一次 git clone，限制为 90 秒；在带随机 UUID 的独立 staging 目录扫描，检查非软链接常规 `<id>/app.yaml`。
- **刷新成功**：把暂存目录移动成独立本地版本工作副本，并在 SQLite 事务中替换 `deploy_catalog_apps` 和版本/日期元数据。
- **刷新失败**：删除失败的临时目录、旧 Git 版本工作副本及 SQLite 上次成功索引保持不变；页面提示错误，已有目录依然可查看。旧版本工作副本暂时保留，避免后续固定版本打包/已安装应用受目录更新影响。
- 无自动拉取、无后台轮询、无 Agent Git 权限。

## REST

| 方法 | 路径 | 权限 | 副作用 |
|---|---|---|---|
| GET | `/api/v1/deploy/catalog` | 已登录、非 webhook 外部模式 | 仅 SQLite 只读，返回 `initialized`、`git_revision`、`fetched_at`、`count`、`items` |
| GET | `/api/v1/deploy/catalog/{id}` | 同上 | 仅 SQLite 只读，返回某应用元数据及只读配置/端口定义 |
| POST | `/api/v1/deploy/catalog/refresh` | **管理员**、非 webhook 外部模式 | 显式拉取 Git、扫描、保存不可变工作副本与索引；失败保留旧索引 |

普通用户 POST 返回 403；Git/权限/网络失败返回统一刷新错误，不把 Token 泄漏给前端。目录未初始化的 GET 返回 `initialized:false`，不暗示有真实可用的商店版本。

## 目录数据与安全边界

- 只索引 `schema_version=1`、`runtime.kind=docker_compose`、`id` 与应用目录一致的正式 `app.yaml`，过滤示例目录、软链接、无效分类、重复显示字段、过大文件及不支持的 YAML 别名/锚点。
- 返回名称、摘要、分类、Compose Project、清单路径、Git commit SHA、清单 SHA-256、运行类型、平台/架构、config key/label/type、端口定义以及 install/update/uninstall 策略名；**不返回实际 `.env`、`.secrets` 值**。
- 目录读取只实现安全的受限 YAML 展示字段提取，**不是完整语义校验，更不是 Shell 信任证明**。即使该目录存在条目，接口也只返回 `install_available:false`、`management_rights:false`。T03-07 打包时必须重新用可信解析器验证固定 Git Revision 下整个 manifest 和每个文件，再给 Agent 受控包；不得使用该只读索引决定任何安装/删除/接管操作。
- 前端允许按分类与关键字本地搜索、查看主入口/配置定义、源 Revision 与 SHA-256；按钮仅刷新、查看，没有“直接安装”。
- 浏览入口不要求有 FnOS 真机；真实 Deploy 私库拉取及带授权运行须在可用持久环境中另行联调。

## 本轮测试

- [x] 已提交后端 Rust 校验器单元测试：未知 YAML、重复元数据、隐藏原始模板不是正式目录、强制只读。
- [x] 已提交 SQLite 回归：初始化已有商店快照时读取不触发 Git；本地 staging 创建失败时不清空旧索引；输出 config 只保留字段定义，不返回 default/秘密值。
- [x] 新的 `deploy_catalog` 模块纳入 `.github/scripts/test-native.sh` 原生必跑回归。
- [ ] GitHub Actions **最新的完整 Docker/Rust/Web/镜像编译通过**；本轮按全局规则不等待。
- [ ] 通过真实只读 GitHub Token 拉取私有 Deploy/main、检查断网保留索引与 Compose 目录在 FnOS 部署环境正确显示。
- [ ] Deploy 仓库自身 Actions 无步骤失败，T03-01～03 的 Python 校验器/正式 YAML CI 仍待恢复。

## 下一任务

T03-06 动态安装表单（**先纯表单与敏感值不回显**，未完成 T03-07 固定版本签名/打包与 T03-08 Agent 验证前不得启用真正“安装”）；并行准备 T03-07 只读已固定 Git 提交包、文件白名单、SHA-256 校验。根据 CI 构建结果先修复当前新增模块的实际编译/运行错误。
