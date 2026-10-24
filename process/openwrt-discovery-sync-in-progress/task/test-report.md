# 文档检查与后续验证记录

## 第1轮：2026-10-24（北京时间）需求与任务拆分

- 操作范围：新增本需求的汇总、8个需求、10个计划、22个任务、公共契约和索引，更新process总索引。
- 代码基线：EasyTier feat/automation-center@dbbade81；LuCI main@d3dbe4f。
- 产品规则：用户已完成三轮关键选择；本轮仅文档拆分。
- 实现进度：0/22，所有任务owner=null/status=todo。

### 本轮实际执行

已只读核对源码、LuCI API、项目AGENTS和CI文档过滤规则；按独占目录并行编写并交叉复审。发现并补入：T07先依赖T15按设备撤销；覆盖保留服务ID/公开归属；原ADB操作在普通预览确认后的续接；失败旧ADB快照不能更新端口；手工服务编辑同一事件链路；resolve/retry只处理当前操作的失败阶段。

目录/链接/编号/依赖与元数据检查结果：已完成。需求8、计划10、任务22；本需求目录46份Markdown，文件集合无缺失或多余；内部相对链接均可解析；22个任务的id、owner、status、plan、硬依赖及联调依赖与统一任务映射一致；硬依赖图无环。三组文档作者已交叉复核范围与接口，无遗留产品问题。

本结果仅证明文档结构及约定一致，不代表代码编译、迁移或任何实际设备行为通过。

### 未执行

- 应用测试、类型检查、lint、前后端构建：未执行，原因是纯文档任务。
- LuCI真实扫描、手机连接/投屏/脚本、公开映射运行态验证：未执行，需后续授权和真实环境。
- 镜像构建、CI手动触发、发布、部署：未执行，本轮不涉及。
- 不安装任何应用依赖，不修改工作流或功能源码。

### 后续验证入口

自动回归见[ODS-T20](task-ods-20-automated-regression.md)，真实环境验收见[ODS-T21](task-ods-21-real-device-acceptance.md)，交付与回退见[ODS-T22](task-ods-22-delivery-and-rollback.md)。每次执行追加对应提交、环境、范围、证据、失败/未执行项，不用本轮文档检查代替功能验证。


## 2026-10-24 · 开发第 1 轮

- 新授权：按 `feat/automation-center@cded5aef` 实施本轮需求，完成代码和模拟联调；真实设备验收另行安排。旧文档中的“仅拆分文档/不含开发”仅适用于此前轮次。
- 当前范围：契约冻结、数据库组级唯一约束和结构化类型、单设备退出屏障先行；再接发现、公开、重连与 UI。
- 实施负责人：Codex；按依赖顺序实施并独占共享文件，不启动其他写入代理。
- 测试：未执行，正在实现；计划覆盖迁移保留、失败旧快照、覆盖回滚、ADB 来源、单机退出、乱序重连及前端交互。
- 实机验收：未执行，按用户决定另行安排。

## 开发第 2 轮：2026-10-24 · 代码与模拟联调

用户明确“先完成代码和模拟联调，实机验收另行安排”。开发基线 `feat/automation-center@cded5aef`，包含技术控制台 `509a787f`。本轮代码、模拟与本地构建已获授权，不沿用拆分轮次的“仅文档”限制。

### 实现范围

迁移 19/20、网关凭据与 LuCI HTTPS、普通发现预览/新建/更新/覆盖/保留服务、ADB 单一来源及一次补发现、公网目标更新及原计划重试、单设备脚本撤销、执行器持久拒收/清理证明、按端点事件重连、原观看意图恢复、技术控制台入口及移动布局。当前由 Codex 顺序完成，无其他并行写入代理。

### 验证环境与命令

Linux x86_64；Rust 1.95，SQLite；Node 24 / pnpm 11.25；Python 3 / httpx 0.28.1；Playwright + 系统 Chromium。测试只连接本机模拟服务，未访问真实 OpenWrt 或 Android。

```bash
# 仓库根目录；当前环境的工具链适配
PATH=/workspace/toolchains/python/cmake/data/bin:$PATH \
RUSTUP_HOME=/workspace/toolchains/rustup CARGO_HOME=/workspace/toolchains/cargo \
RUSTFLAGS='-C link-arg=-fuse-ld=bfd' \
PROTOC=/workspace/toolchains/protoc/bin/protoc \
LIBCLANG_PATH=/workspace/toolchains/python/clang/native \
BINDGEN_EXTRA_CLANG_ARGS='-I/usr/lib/gcc/x86_64-linux-gnu/14/include' \
/workspace/toolchains/cargo/bin/cargo test -p easytier-web

# frontend-lib 已在本轮先构建；最终验证采用现有 CI 构建入口
PATH=/workspace/toolchains/protoc/bin:$PATH pnpm --dir easytier-web/frontend build:ci

# automation-executor 目录
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v

# easytier-web/frontend 目录
node --test tests/discovery.test.mjs
```

### 已覆盖的行为

| 范围 | 证据与结果 |
| --- | --- |
| 迁移/旧库 | 旧类型和名字不会自动变为手机或 ADB；保留服务 ID、旧连接及执行历史，外键完整；拒绝有损降级。 |
| 网关配置 | 密码含首尾空格、换行、中文及特殊字符仍精确保存；读取不回显，保留/清除语义正确；HTTPS 信任校验、错误密码、认证/能力检查不扫描。 |
| HTTP 联调 | 真实 Axum 路由和账号层调用 HTTPS LuCI mock；他人请求拒绝；一次 ADB 扫描只补一次普通扫描，只导入缺失 IP；仍缺失的手机跳过，历史读取/原请求重放不发 RPC。 |
| 预览与应用 | 读取不写资产；缓存来源标记保留；人工名/手机类型/未知字段保留；同组唯一、跨组可重复；服务变更和到期预览拒绝；单行覆盖失败回滚。 |
| ADB/公开 | error、busy、无快照和错误来源旧数组不写端口；多端点显式选择；同端口不发新事件；公网监听口不变而目标端口更新，覆盖后服务 ID/公开记录归属正确。 |
| 续接 | 补发现部分确认只终结已处理 IP；其他行仍可处理；继续原快照，无第二次扫描。 |
| 退出/重连 | 两台手机只撤销 A，B 继续第二步；派发前退出 A 不影响 B；清理失败禁止新连接；仅重试清理不连接；成功连接重复重试不重复执行；新身份等待前驱清理。 |
| 执行器 | 拒收跨重启保留，旧准备取消完成后才定向断开；不全局断开、不伤害新目标；身份变化不能创建/关闭旧会话；未知或未退出进程不冒充清理成功。 |
| 浏览器 | 真正构建后的 Vue 页面在 Chromium 运行，API/媒体信令使用模拟；页面空闲快进两分钟无轮询，保存不扫描，预览须点击应用，多端点确认不重扫；390px 对话框不超宽；重复事件、手动关闭及切页不重新开屏，不重发输入/脚本。 |

### 调试过程与纠正

此前全量回归 390 通过、2 失败、2 忽略：新增测试夹具重复创建同名账号、把随机行 ID 的顺序误当成 IP 顺序，均已修正。HTTP 夹具还改为与生产一致地复用同一运行时凭据密钥。HTTPS mock 使用合法的服务器证书。浏览器夹具补齐完整导航和悬浮控制菜单操作。业务复审补强了补发现空结果终结、覆盖前驱清理、执行器会话身份、多个旧目标的独立清理记录，以及公开重试保留选中目标。

### 未执行与边界

- 两个仓库原有高连接数压力测试默认忽略：4097 TCP 连接与 1000 管理连接，本轮未额外启动。
- Python 替换了媒体管理器和设备命令；浏览器模拟了 EventSource/WebRTC/WebSocket。真实视频首帧、Android 配对/进程退出、控件动作未验证。
- OpenWrt 真机 ACL/证书、fnOS 容器、公开节点真实 RPC 应用与读回、ARM 媒体执行器、镜像构建发布、生产部署和回退演练：未执行，按用户安排留给 T21/T22。
- 前端构建沿用已有的大 chunk 和 Browserslist 数据陈旧提示；未为消除提示修改依赖锁或拆包策略。

最终结果与日志在下方收口条目登记。

### 最终收口结果

本轮最终源代码的检查结果（本记录之后仅补文档、证据与提交）：

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| `cargo test -p easytier-web` | **393 passed / 0 failed / 2 ignored**，46.79 秒 | [完整日志](artifacts/automated/rust-tests.log) |
| Python journal/runtime 模拟 | **8 passed**，0.299 秒 | [完整日志](artifacts/automated/executor-tests.log) |
| `pnpm build:ci` | **通过**，Vue 类型检查与 Vite 生产构建完成 | [构建日志](artifacts/automated/frontend-build.log) |
| Chromium 交互 | **3 passed / 0 failed** | [完整日志](artifacts/automated/browser-tests.log) |
| 桌面/移动样式 | 复用技术控制台；移动对话框宽度断言通过 | [桌面](artifacts/automated/discovery-desktop.png)、[移动](artifacts/automated/discovery-mobile.png) |
| `git diff --check` | 通过 | 提交前执行，无空白错误 |

ODS-T01–T20 完成；ODS-T21 按用户指示另行实机安排；ODS-T22 的升级/回退材料已提供，发布部署未执行。本轮没有真实设备验收通过结论。

## 开发第 3 轮：2026-10-24 · 服务管理卡片布局

- 用户要求服务管理一行展示多个服务卡片。基于 `28b46da3` 调整 `ServiceManagement.vue`，保留技术控制台主题。
- 服务列表采用自适应多列网格；卡片依次展示图标/名称/菜单、访问地址/所属设备和导航开关/未检测标记。加载占位也采用卡片排列；长地址省略展示且保留完整 title。
- 现有浏览器用例同步卡片选择器。前端构建及相关浏览器回归通过，结果如下。后端、执行器和实机测试未重复执行，本轮只改前端展示。

### 本轮验证结果

- `PATH=/workspace/toolchains/protoc/bin:$PATH pnpm --dir easytier-web/frontend build:ci`：通过（类型检查和生产构建），[构建日志](artifacts/automated/service-cards-build.log)。
- 前端目录运行 `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/usr/bin/chromium WEB_SCREENSHOT_DIR=/tmp/service-cards-screenshots node --test --test-name-pattern='device-owned navigation works|global editing updates|administrator catalogs|asset-backed 3D map and services' tests/network-space.test.mjs`：**4 passed / 0 failed**，验证编辑/删除、导航开关、账号范围及桌面/移动布局，[浏览器日志](artifacts/automated/service-cards-browser.log)。
- 已查看截图：1440px 桌面一行 3 张卡片，390px 移动端自动单列，两种宽度均无页面横向溢出。[桌面效果](artifacts/automated/service-cards-desktop.png)、[移动效果](artifacts/automated/service-cards-mobile.png)。
- `git diff --check` 通过。沿用已有构建体积/Browserslist 提示；未新增业务逻辑或测试用例，仅同步现有测试的卡片选择器。
- 本轮提交至 `feat/automation-center`，使用 `[skip ci]`，未发布镜像或部署。此前后端与执行器测试结果仍只对应前一轮代码范围。

## 开发第 4 轮：2026-10-24 · 服务导航卡片网格

- 用户确认将“服务导航里面的列表”做成卡片形式，基于 `aa83b7de` 修改 `ServiceNavigation.vue` 及 `ServiceCard.vue` 布局。
- 服务统一显示在自适应多列网格中，沿用已有服务卡片及控制台主题；保留节点/设备筛选和卡片内的设备归属。网格使用保留空列的布局，单个筛选结果也维持普通卡片宽度。
- 加载占位同步使用卡片网格，同行卡片的底部地址栏对齐。前端构建及浏览器交互和桌面/移动布局检查通过；后端、执行器及实机验证未重复执行，本轮仅涉及前端布局。

### 本轮验证结果

- `PATH=/workspace/toolchains/protoc/bin:$PATH pnpm --dir easytier-web/frontend build:ci` 通过，包含 Vue 类型检查和生产构建；[构建日志](artifacts/automated/navigation-cards-build.log)。
- 前端目录执行 `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/usr/bin/chromium WEB_SCREENSHOT_DIR=/tmp/navigation-cards-final node --test --test-name-pattern='device-owned navigation works|service navigation locates|asset-backed 3D map and services' tests/network-space.test.mjs`：**3 passed / 0 failed**。覆盖服务访问、搜索/移出导航、设备定位和中英文页面、桌面/移动布局；[浏览器日志](artifacts/automated/navigation-cards-browser.log)。
- 已查看截图，1440px 桌面统一 3 列、卡片地址栏对齐；390px 移动端自适应单列，无页面横向溢出。[桌面效果](artifacts/automated/navigation-cards-desktop.png)、[移动效果](artifacts/automated/navigation-cards-mobile.png)。
- `git diff --check` 通过。本轮没有新增测试用例；沿用前端构建体积和 Browserslist 提示。提交到 `feat/automation-center`，使用 `[skip ci]`，未发布镜像或部署。

## 开发第 5 轮：2026-10-24 · 合并地图交互并开启构建

- 用户明确要求将 `feat/map-detail-interactions` 合并到 `feat/automation-center`，并开启构建。目标基线 `3f8baf31`，来源分支最新提交 `c23534da`，共同基线 `509a787f`。
- `git merge --no-ff --no-commit origin/feat/map-detail-interactions` 自动合并成功，无文本冲突。地图缩放、离线行政边界、同址节点/线路选择、网关自身设备合并与可辨识度调整已引入；目标分支服务管理/导航卡片布局及新版选择器保留。
- 前端类型检查和生产构建通过，单元测试 34/34 通过，关键浏览器交互回归 7/7 通过。后端与执行器代码没有合并改动，本轮未重复其测试；实机验收未执行。
- 沿用 `.github/workflows/node-attributes-image.yml` 的 push 触发器，合并提交不带跳过 CI 标记，计划通过推送自动启动 `linux/amd64` GHCR 构建。未修改工作流或部署配置，未部署实机。
- 当前环境 `api.github.com` 不在网络允许列表，已改为读取 GitHub Actions 网页确认状态；Git 推送与 Actions 网页可访问。具体运行记录在推送后补充。

### 合并后本地验证

- `PATH=/workspace/toolchains/protoc/bin:$PATH pnpm --dir easytier-web/frontend build:ci`：通过；[构建日志](artifacts/automated/map-merge-build.log)。
- 前端目录 `pnpm test:unit`：**34 passed / 0 failed**；[单元测试日志](artifacts/automated/map-merge-unit.log)。
- 前端目录 `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/usr/bin/chromium WEB_SCREENSHOT_DIR=/tmp/map-merge-screenshots node --test --test-name-pattern='device-owned navigation works|global editing updates|globe reaches|co-located and nearby|a gateway self device|multiple gateways remain|asset-backed 3D map and services' tests/network-space.test.mjs`：**7 passed / 0 failed**，约 45.92 秒；[浏览器日志](artifacts/automated/map-merge-browser.log)。
- 验证服务访问/卡片操作、服务编辑删除、区域与城市缩放/行政标签、同址/邻近节点与线路选择、单/多网关自身设备及桌面/移动布局。使用模拟 API 和 Chromium WebGL，无真实设备写入。
- `git diff --check` 通过。保留来源分支已有的离线行政数据大包提示，以及现有 Browserslist/pnpm 提示；未调整依赖、构建脚本或数据精度。
