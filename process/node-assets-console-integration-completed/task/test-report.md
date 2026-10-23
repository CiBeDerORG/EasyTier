# 执行与验证记录

## 第 1 轮：2026-10-23 UTC

范围：网络地图切换到注册节点和资产设备，服务管理/导航对接资产服务，设备管理菜单提升一级。

已执行：阅读现有页面和接口、核对注册节点与实例协议、并行划分文件写入职责。已确认旧地图依赖 `/networks` 与独立地图配置，旧服务管理读取 `/services`，设备内服务实际位于 `/assets`。

当前状态：实现已完成，用户明确授权“执行本轮针对性验证”。此前分支及镜像构建结果不代表本轮通过；本轮代码尚未提交、推送或构建镜像。

实现内容：

- 地图读取 `/machines` 注册节点及 `geo_location`，每个运行实例独立采样；按来源路由的实例 UUID 与 peer ID 唯一匹配，拒绝按重名或重叠 IP 猜测连接。
- 坐标编辑复用节点属性弹窗；空间设备来自资产设备及网关组，支持真实父子关系和仅作示意的户型布局。
- 服务管理、导航和地图关联服务读取同一份资产服务目录；导航 PATCH 只修改导航状态，完整编辑复用设备服务弹窗。
- 新增资产服务导航字段及迁移，旧编辑请求省略字段时保留状态，清除 URL 时关闭导航。原独立服务和地图记录保留在原表，未自动复制或删除。
- 设备管理为网络节点下方一级菜单，地图/服务链接可以定位设备所在分页并展开其服务，已删除设备有明确反馈。

已执行验证：

- 多实例拓扑、设备空间适配及原地图逻辑：23/23 通过。
- 首次前端构建因默认 PATH 未包含本地 protoc、依赖包装器尝试在线解析版本失败；使用 `/tmp/easytier-protoc/bin` 后，完整共享库及应用类型检查/生产构建通过。
- 独立源码审查发现并修复：轮询覆盖其他节点的资产网关角色；无关失效实例污染健康路径；暂停/隐藏产生的过期状态被计时器错误清除；异步加载大量设备后镜头未重新适配。修复后的 `pnpm build:ci` 通过。
- 后端 `cargo test --locked -p easytier-web --no-fail-fast`：372 项中 370 通过、0 失败、2 项既有大连接压力测试忽略。覆盖服务目录、导航开关、权限、URL 校验、发布关系及两侧父分支数据库升级。
- 完整后端测试结束后追加“删除服务再重放创建请求，返回 410 且不复活服务”的断言，定向测试 1/1 通过；未重复整个后端套件。
- 网络地图与资产服务浏览器回归：14/14 通过，包括无集中式网络的注册节点地图、坐标保存、多实例直连/中继/失效、真实网关关系、服务创建/编辑/删除/导航同步、账户隔离、自动刷新、WebGL 降级、侧栏收起及桌面/手机布局。
- 菜单及设备定位浏览器回归：2/2 通过；节点属性浏览器回归：15/15 通过。设备定位截图专项重复其中 1 个用例并通过，不重复计入唯一本例数。
- 人工复核桌面与手机截图后，补齐服务页面 3 处默认筛选文案。最终 `pnpm build:ci` 通过；新增 100 台设备异步加载和受影响视觉用例定向 2/2 通过。前者先挂载空场景再加载设备，验证外圈 97–100 号设备完整入镜；后者复核桌面/手机筛选文案、服务管理布局和家庭户型切换。
- 网络地图/服务套件共 15 个不同用例，加上菜单/设备及节点属性的 17 个不同用例，本轮浏览器合计 32 个不同用例通过；截图和视觉重复运行未重复计数。已人工查看 100 台设备、家庭户型、服务导航、服务管理及设备菜单的截图。
- 最终 `git diff --check` 通过，无浏览器测试失败或未解决的阻塞问题。

验证边界：浏览器使用可控 API 数据检查交互和多实例场景，后端使用测试数据库检查真实 SQL、HTTP 权限与迁移。未连接生产节点或修改生产数据库。完整前端构建存在依赖数据过期及 bundle 体积提示，构建成功。

证据：

- 单元测试：`/tmp/easytier-node-assets-unit.log`。
- 前端构建：`/tmp/easytier-node-assets-frontend-build-local-protoc.log`、`/tmp/easytier-node-assets-frontend-final-build.log`、`/tmp/easytier-node-assets-frontend-visual-build.log`。
- 后端命令、结果与限制：`/workspace/scratch/easytier-node-assets-integration/backend-validation.md`；完整及重放日志：同目录 `backend.log`、`backend-replay.log`。
- 地图/服务浏览器日志：`/workspace/scratch/easytier-node-assets-integration/network-space.log`、同目录 `network-space-visual.log`；截图位于同目录，包括 `asset-node-space-100-devices-auto-fit.png`、`asset-node-home-cn-dark-desktop.png`、`asset-service-navigation-cn-dark-desktop.png`、`asset-service-management-cn-dark-mobile-viewport.png`。
- 设备菜单/定位及节点属性记录：`/workspace/scratch/integration-device-browser/QA.md`，日志与截图在同目录。

## 第 2 轮：推送与镜像更新（2026-10-23 UTC）

用户明确要求“推送并更新镜像”。本轮提交已经通过上述验证的实现与记录，沿用现有分支及镜像工作流，不修改构建配置。

- 发布分支：`feat/node-attributes-tech-console`。
- 目标镜像：`ghcr.io/cibederorg/easytier:feat-node-attributes-tech-console`，平台 `linux/amd64`；同时保留提交 SHA 标签。
- 已核对自动 push 触发条件、发布步骤及本轮差异；`git diff --check` 通过。
- 首次推送被非快进保护拒绝：远端新增 `4e778010`（部署管理及自动化中心）。已合入远端更新，保留其全部功能；两个冲突文件取功能并集，保留双方测试脚本及迁移，未强制覆盖远端。
- 合并后的 `pnpm build:ci` 通过；三条父分支数据库升级测试 3/3 通过；导航/服务目录/创建重放/部署代理定向后端测试 12/12 通过（其中包含重复的地图父分支升级用例，不作为额外唯一本例）。
- 最终合并产物的菜单/设备定位浏览器 2/2、部署管理浏览器 2/2 通过，已目视菜单及手机布局。部署浏览器使用模拟 API，未触发真实安装。
- 合并验证日志：`/tmp/easytier-node-assets-release-merge-frontend.log`、`/tmp/easytier-node-assets-release-merge-upgrade.log`、`/tmp/easytier-node-assets-release-merge-services.log`；浏览器证据：`/workspace/scratch/easytier-node-assets-release/`。
- 未重复完整应用回归；本轮新增远端代码经上述定向验证，远端发布流程还将执行 Web/原生构建、容器启动测试和镜像发布核验。
- 提交、远端构建及发布结果：执行中，尚未报告成功；完成后补充准确提交、Actions 地址、digest 和拉取结果。
