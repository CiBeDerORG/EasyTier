# 分支整合测试记录

## 第 1 轮：2026-10-23（UTC）

范围：合并 `9ed4d609` 和 `c5843c2d`，解决六个冲突文件，补全拓扑路由和地图自动刷新衔接。

- 已执行：双方提交与共同基点检查；确认工作区起始干净；远端分支拉取；六处冲突人工融合。
- 已执行：使用可写临时缓存目录运行 `pnpm install --frozen-lockfile`，通过；未改动锁文件。
- 前端库及应用类型/构建：通过；补齐节点分支弹窗 pass-through 类型所需的泛型参数后，应用类型检查和生产构建通过。
- 共享前端库：57/57 通过；地图/服务逻辑：11/11 通过；网络配置映射：10 项断言通过。
- 浏览器回归：fixture 已适配当前节点属性契约，正在并行验证控制台、节点属性、地图/服务，使用独立端口避免冲突。
- Rust 后端及数据库升级验证：生产代码编译通过，367 项测试执行中；包含从双方旧数据库升级的数据保留验证。
- 推送、部署及远端 CI：未执行，本轮用户仅要求分支整合。

环境说明：首次安装使用默认 pnpm 目录失败，改用 `/tmp/easytier-pnpm-*` 可写缓存后成功。首次后端编译需补充当前机器的 GCC builtin include 路径；正在使用对应编译环境重试，不改动项目代码规避编译问题。

待测：双侧迁移兼容，节点属性保存与失败恢复，科技风界面、地图/服务、用户权限入口、自动刷新和手机布局。此前两分支的历史通过记录不作为本次合并的通过证据。

当前日志：`/tmp/easytier-merge-install.log`、`/tmp/easytier-merge-build.log`、`/tmp/easytier-merge-build-app.log`、`/tmp/easytier-merge-lib-tests.log`、`/tmp/easytier-merge-map-unit.log`、`/tmp/easytier-merge-config-mapping.log`、`/tmp/easytier-merge-backend.log`。浏览器证据目录：`/workspace/scratch/easytier-merge-browser/`。

## 第 2 轮：2026-10-23（UTC）最终结果

所有本轮发现的问题已解决。没有改变生产 API、权限或两套资产/地图存储模型来迁就测试。

| 验证 | 实际结果 |
| --- | --- |
| 锁定依赖安装 | `pnpm install --frozen-lockfile` 通过，锁文件无额外变化 |
| 前端库、应用类型和生产构建 | 通过；`DialogPassThroughMethodOptions<unknown>` 修复了父分支缺失的泛型参数 |
| 共享组件库 | 57/57 通过 |
| 地图/服务逻辑 | 11/11 通过 |
| 网络配置映射 | 10 项断言通过 |
| 控制台浏览器 | 首轮 34/36；修正两处测试定位/等待后，定向 2/2 通过，36 个用例均已验证 |
| 节点属性浏览器 | 首轮 14/15；经纬度用例调整真实键盘输入和 Leaflet 就绪判断后定向 1/1 通过，15 个用例均已验证 |
| 地图/服务浏览器 | 15/15 通过，含自动刷新暂停、手动刷新和恢复 |
| 最终截图复核 | 地图手机视觉流程 1/1、节点桌面/手机流程 2/2 再次通过 |
| 原有 Rust 后端测试 | 363 项通过，2 项既有大连接压测忽略 |
| 新增双侧数据库升级 | fixture 用户 ID 修正后定向 2/2 通过 |
| 独立源码审查 | 两侧 API 方法、权限、路由、迁移、异步请求隔离及冲突解决未发现具体遗漏 |
| 差异检查 | `git diff --check origin/feat/node-attributes-main` 通过 |

浏览器共覆盖 66 个不同用例，通过首轮加定向复测完成；没有声称修正后再次跑过完整三套。后端原有 363 项与新增 2 项均通过，新增迁移修正后仅定向重跑，没有声称再次运行完整 367 项。

### 首轮发现与处理

- 新增迁移 fixture 使用了初始迁移已占用的用户 ID；改为独立测试用户后，从两个父分支迁移历史升级均成功。验证地图 JSON、导航服务、节点属性、经纬度、资产设备/服务保留，并检查重复启动幂等和外键完整性。生产迁移未作额外修改。
- 网络协议选择的端口输入断言早于 Vue 渲染完成；等待原预期控件数量后再次验证所有协议和请求值。
- 资产菜单名称来自最新 `node-console.yaml` 的覆盖，测试改为当前 `Device list`，继续验证管理员入口和权限收回。
- PrimeVue 数字框需要真实键盘事件更新模型，Leaflet pane 本身没有可见边界框；测试使用键盘输入和可见地图控件，仍完整断言坐标成对校验、逆编码、地图点选和 WGS84 保存。
- 手机整页截图曾保留前序滚动/键盘焦点；归位后补拍视口截图，确认自动刷新开关所在两行顶栏正常，没有遮挡或横向溢出。
- 相对科技风父分支的整体 whitespace 检查会报告节点分支原有文档/日志末尾空行；这些历史产物保持原样。本轮相对节点父分支的全部新增修改通过差异检查。

### 最终证据与限制

浏览器日志及截图：`/workspace/scratch/easytier-merge-browser/`，含 `dashboard.log`、`dashboard-recheck.log`、`node-attributes.log`、`geo-final.log`、`network-space.log`、`map-visual-final.log`、`node-visual-final.log`。

后端首轮日志：`/tmp/easytier-merge-backend-before-fixture-fix.log`；最终升级日志：`/tmp/easytier-merge-upgrade-final.log`。后端在本机 Rust/protoc 和可写临时缓存下执行 `cargo test --locked -p easytier-web --no-fail-fast`，最终定向筛选 `migrator::merge_upgrade_tests`。

已目视地图手机视口、服务导航桌面、节点卡片、手机节点列表与属性弹窗。浏览器使用模拟 API；数据库升级使用两侧真实迁移历史及模拟已存数据，不代表已连接生产设备或升级用户线上数据库。构建仍有已有的大分块提示，本次未做额外包体拆分。未推送、部署或触发远端 CI。

最终状态：通过。

## 第 3 轮：推送与镜像发布（2026-10-23 UTC）

用户已明确授权 push 并生成镜像。合并提交为 `f675ceac`；本轮仅调整已有镜像工作流，使新整合分支能够触发相同的构建、容器启动测试和 GHCR 发布流程。

- 支持分支：原 `feat/node-attributes-main` 与新 `feat/node-attributes-tech-console`。
- 新分支镜像：`ghcr.io/cibederorg/easytier:feat-node-attributes-tech-console`；旧分支标签保持原值。
- 默认平台：`linux/amd64`，沿用现有 push 构建默认值；手动工作流仍支持选择 arm64。
- 保留原生制品来源校验、发布前容器测试、SHA 标签、digest 核验和匿名拉取检查。
- 不触发旧的独立重复验证工作流；主发布流程已包含镜像验证。
- 本轮不重跑未修改的应用回归；仅验证工作流配置，随后观察远端构建和发布结果。

执行状态：工作流配置已完成，等待静态检查、推送及远端执行结果。当前 GitHub API 直连被环境网络限制拒绝，Git 远端仍可访问；将通过已配置凭据和可用访问方式完成推送及状态核对。
