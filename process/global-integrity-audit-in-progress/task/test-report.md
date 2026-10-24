# 全局完整性测试与审计报告

## 第 1 轮 · 2026-10-24

基线 `c4ad8e06`。用户本轮明确授权全局完整性测试；真实设备按此前安排另行验收。本轮不修改业务代码，测试报告和复现证据在此收口。

**结论：尚未形成完整交付闭环。** 已接通的资产、服务、发现和自动化 HTTP 链路有较多通过证据；执行器依赖、部署接线、Agent 交付、手动触发要求和两个页面操作流程仍有明确缺口。不能把单元测试通过解释为真实手机画面和全平台安装已验收。

## 已确认的问题

### F01 · P1 · 手机执行器的固定媒体依赖无法直接安装

- **触发**：按当前 `automation-executor/requirements.txt` 在独立 Python 3.12 环境安装依赖。
- **结果**：`av==14.4.0` 只有源码包，没有 wheel；安装报缺少 libavformat/libavcodec 等开发库，并明确要求 FFmpeg 7。`pip download --only-binary=:all: --no-deps av==14.4.0` 也失败。已通过 PyPI 官方元数据核实发行文件，仅 `av-14.4.0.tar.gz`。
- **影响**：Dockerfile 使用 `python:3.12-slim-bookworm`，只准备常规运行库，没有 C 编译器、pkg-config 和 FFmpeg 7 开发环境；媒体执行器不能按现有材料完成依赖安装。此次未执行完整 Docker build，不把依赖失败伪称为容器构建日志。
- **位置**：[依赖第 6 行](../../../automation-executor/requirements.txt)、[Dockerfile](../../../automation-executor/Dockerfile)。
- **证据**：[安装失败](artifacts/executor-deps.log)、[仅 wheel 检查](artifacts/av-wheel.log)、[官方发行文件](artifacts/av-distribution.json)。
- **建议**：选择与 aiortc 匹配且提供目标平台 wheel 的 PyAV 版本，或完整提供 FFmpeg 7 编译链；锁定传递依赖，再实测镜像启动和首帧。不能仅改成任意最新版就宣布完成。

### F02 · P1 · 自动化执行器没有接入现有 deploy 安装链路

- `automation-executor/compose.yaml` 是单独手工部署材料；deploy 的应用目录没有此项目，EasyTier Compose 也没有传入 `EASYTIER_AUTOMATION_RUNNER_URL/TOKEN`。当前 GHCR 工作流构建的是 EasyTier Web/Core 镜像，不含执行器镜像。
- **影响**：只在页面安装/更新 EasyTier 或拉取 `feat-automation-center` 镜像，自动化中心仍会显示执行器未配置；无法经现有 Agent 一次完成安装、配置主服务与回调、保留 ADB 密钥/数据卷并启动验收。
- **位置**：[执行器部署](../../../automation-executor/compose.yaml)、[deploy EasyTier 配置](../../../../deploy/easytier/deploy/docker-compose.yaml)、[Agent 目录](../../../../deploy/agent/catalog.json)、[镜像工作流](../../../.github/workflows/node-attributes-image.yml)。
- **建议**：增加执行器安装项目、主服务配置传递、可达的回调地址和一次性“检查接入”操作。执行器保持独立进程；明确 x86_64 首版范围，不能把 OpenWrt ARM Agent 支持等同于 ARM 媒体执行器支持。

### F03 · P1 · deploy Agent 仍未形成可从仓库复现的交付

- 审计时 `/workspace/deploy` 位于 `main@b73694d`；`agent/`、`docs/`、`.github/workflows/agent-check.yml` 为未跟踪文件，`deploy.sh` 和 README 为未提交修改；`git ls-tree HEAD agent` 无结果。
- **影响**：当前工作区可以构建并完成跨仓库模拟安装，但从该仓库已提交的 main 干净检出，无法得到 Agent、安装器及其 CI。不能把 EasyTier 分支推送当成两个仓库均已交付。
- **证据**：[分支、提交与工作区记录](artifacts/deploy-delivery.txt)。本轮没有提交、丢弃或覆盖这些既有 deploy 改动。
- **建议**：把 Agent 和 deploy 入口变更形成独立可审阅提交，提供带校验值的 amd64/arm64/armv7 制品与源码版本对应关系，再验证干净检出安装。

### F04 · P1 · “全局仅点击触发”尚未落实

- 全新浏览器默认开启 `webConsole.autoRefresh`；普通页面仍定时刷新，网络地图另有 15 秒 interval，导航有 10 秒刷新。
- **复现**：真实隔离后端、没有点击，推进 30 秒浏览器时钟后产生 **15 次** summary/machines/networks/blocked-devices 请求。新 Agent 页和自动化页的空闲无轮询测试通过，但不能代表全站。
- **位置**：[默认刷新偏好](../../../easytier-web/frontend-lib/src/modules/polling.ts)、[导航刷新](../../../easytier-web/frontend/src/components/MainPage.vue)、[地图刷新](../../../easytier-web/frontend/src/components/NetworkMap.vue)。
- **证据**：[UI 复现日志](artifacts/ui-gaps.log)、[请求明细](artifacts/result.json)。
- **建议**：按用户明确要求统一去除空闲周期刷新，保留页面按钮触发。活动手机控制 WS 的 20 秒权限续验及 60 秒租期是另一项行为，已单独询问是否允许作为例外；未把无回复当作批准。EasyTier 底层组网心跳也不能宣称不存在。

### F05 · P2 · 未保存脚本切换页签后无提示丢失

- **复现**：在脚本库填写名称并添加步骤 → 点击执行记录 → 返回脚本库；名称变空、步骤归零，没有确认弹窗。
- **原因**：`AutomationCenter.vue` 用 `v-if/v-else-if` 销毁脚本编辑器，草稿只在子组件本地；`dirty` 仅用于标识和禁用运行，没有离开保护。同样需要检查“新建脚本”“选择另一脚本”“导入录制”的覆盖行为。
- **位置**：[页签卸载](../../../easytier-web/frontend/src/components/AutomationCenter.vue)、[草稿状态](../../../easytier-web/frontend/src/components/automation/ScriptLibrary.vue)。
- **证据**：[复现程序](ui-gap-probe.mjs)、[日志](artifacts/ui-gaps.log)、[切换前](artifacts/script-before.png)、[切换后](artifacts/script-after.png)。已人工查看截图。
- **建议**：按账号和脚本保存未提交草稿，在离开/覆盖前提供保存、丢弃或返回选择；不要把实际输入参数自动持久化进草稿。

### F06 · P2 · 管理其他账号节点属性的页面入口断开

- 真实浏览器可作为管理员选择另一个账号的节点并管理设备，但 `NodeAssetsPage.vue` 已无节点属性编辑入口。旧 R05 回归在该入口失败；此项没有改成跳过或绿色。
- 目前 `DeviceAttributesDialog` 在网络节点/地图路径使用，本人的机器列表不能替代目标账号节点目录。隔离实测管理员 `/api/v1/machines` 为 0 项，而目标账号 `/api/v1/assets/nodes` 为 3 项。
- **影响**：后端节点 metadata 接口仍可用，但管理员无法从目标账号设备管理上下文编辑其节点地点/型号等属性；此前的对应 UI 验证不能沿用。
- **位置**：[当前页面](../../../easytier-web/frontend/src/components/NodeAssetsPage.vue)、[属性弹窗](../../../easytier-web/frontend/src/components/DeviceAttributesDialog.vue)。
- **证据**：[真实浏览器 7 子场景中的 1 个失败](artifacts/binding-browser-final.log)、[管理员管理目标账号时的页面](artifacts/admin-node-entry.png)。其余来源冲突、删除恢复、网络异常保留、返回滚动和管理员撤权子场景继续执行并通过。
- **建议**：在目标账号上下文提供统一属性编辑入口，并确保别名写入同样带正确账号作用域。若产品已取消这项管理能力，需要明确更新需求，不能只删测试。

### F07 · P2 · 执行失败步骤到脚本编辑器的定位未对接

- [产品流程第 6 节](../../automation-center-in-progress/require/require-06-product-flow.md)要求从结果点击失败步骤定位当前草稿，并识别步骤已变更/删除。
- `RunHistory.vue` 的步骤列表只有结果文本与截图按钮，没有打开脚本/步骤的事件；父组件也没有接收此定位的接口。后端稳定 `step_id` 和执行快照已经存在，前端操作链未接上。
- **位置**：[结果步骤展示](../../../easytier-web/frontend/src/components/automation/RunHistory.vue)、[父组件接线](../../../easytier-web/frontend/src/components/AutomationCenter.vue)。本项为需求到源码的确定缺口，未伪造按钮点击测试。
- **建议**：以 script_id/revision/step_id 定位；快照与当前草稿不一致时明确提示，不能按数组下标误定位。

### F08 · P2 · 执行器健康响应高估当前能力

- 真实 FastAPI/Runtime/SQLite 启动后，即使没有 adb、aiortc/PyAV，health 仍返回 `state=ready, adb=true, webrtc=true`；实际 inspect 返回 `adb_unavailable`，offer 返回 `media_dependencies_missing`。
- **位置**：[Runtime.health](../../../automation-executor/automation_executor/runtime.py)。
- **证据**：[真实 HTTP 探测](artifacts/executor-http.log)。这轮为绕开 F01 继续审查 HTTP 契约，只安装了固定 HTTP/控件依赖；未替换 MediaManager，也没有声称完整媒体依赖已安装。
- **建议**：区分“支持此协议”与“本机依赖就绪”，在用户点击检查时验证依赖，返回可操作错误；不需要增加周期探测。

## 原始目标仍未闭环的范围

| 范围 | 实际状态和影响 | 下一步 |
| --- | --- | --- |
| G01：纯净 fnOS 只装 Agent 即可部署容器应用 | [Agent tasks.go](../../../../deploy/agent/tasks.go)在缺 Docker/Compose 时直接失败；[README](../../../../deploy/agent/README.md)明确尚未实现 fnOS Docker 初始化。Agent 本身可以原生启动，但初始运行环境仍需人工准备。 | 明确支持的 fnOS 版本，增加点击触发的运行环境检查与官方安装引导/适配；无法据 x86 架构猜定系统版本。 |
| G02：客户端连接服务端后的首次可达 | Agent 当前为被动 HTTPS 监听，没有注册或反向命令通道。服务端必须先能访问其地址；位于 NAT 后的全新 fnOS 不会因安装 Agent 就自动接入远端服务端。 | 明确局域网、已有 EasyTier 或显式端口映射的初始接入流程；如需反向接入，另定连接中断后仅手动恢复的契约。 |
| G03：发现服务进入统一服务管理（用户本轮已确认，P1 未实现） | Agent 结果存于 `deploy_agents.services_json`；服务管理/导航读 `asset_services`，没有确认导入及设备归属映射。[现有说明](../../../docs/deploy-agent-integration.md)原本把自动归属映射排除在第一版之外；用户本轮明确选择手动确认导入。 | 必须提供“确认导入”按钮，用户选择设备归属、服务与访问地址后写入资产。发现不自动入库；导入后复用同一资产服务进入服务管理及导航。当前没有实现，不能标完成。 |

## 本轮测试结果

| 范围 | 实际结果 | 证据/边界 |
| --- | --- | --- |
| Rust Web 全套 | **393 passed / 0 failed / 2 ignored** | [日志](artifacts/rust.log)，两个原有大连接数压力测试未额外启用。 |
| Rust core 全套默认库测试 | **847 passed / 0 failed** | [日志](artifacts/core-proto.log)。 |
| Rust proto | **33 passed / 0 failed / 2 ignored** | 同上，忽略项为既有 proof 性能测试。 |
| 原生 easytier 库 | **未通过，进程 SIGABRT 中止** | [原始日志](artifacts/native.log)。822 项计划执行，截断前记录 269 ok / 23 FAILED，其余没有完成结果；详见 V02，不合并进通过总数。 |
| Web 当前源码编译 | **通过** | [嵌入前端二进制构建](artifacts/web-build.log)。隔离 HTTP 和浏览器使用本轮重新构建的二进制。 |
| 前端完整构建 | **通过** | [日志](artifacts/frontend-build.log)，含 frontend-lib 代码生成/类型检查及 frontend 类型检查/生产构建。 |
| 地图/拓扑单元测试 | **34/34** | [日志](artifacts/frontend-unit.log)。 |
| 配置组件/共享逻辑 | **57/57**，另 **10/10** 配置序列化断言 | [Vitest](artifacts/config-ui.log)、[配置契约](artifacts/network-config.log)。 |
| Go Agent | **14 项通过**，`-race -count=1` | [非缓存重跑日志](artifacts/agent-final.log)，[amd64/arm64/armv7 交叉编译](artifacts/agent-build.log)通过。编译不等于目标平台运行。 |
| Python 既有回归 | **8/8** | [日志](artifacts/executor.log)，原测试替换媒体及设备动作。 |
| Python 固定完整依赖 | **失败** | F01；[日志](artifacts/executor-deps.log)。 |
| Python 实际 HTTP/runtime/journal | **通过并复现 F08** | [日志](artifacts/executor-http.log)，验证身份、缺失能力、会话关闭、拒收记录及重启。 |
| Rust↔Python 实际 HTTP 联调 | **通过** | [日志](artifacts/automation-integration.log)：真实数据库、请求、回调与执行器，仅将 adb 命令替换成隔离夹具；未准备控件时的失败正确保留。没有视频成功或手机动作成功结论。 |
| Web↔Go Agent↔deploy.sh | **通过** | [日志](artifacts/agent-integration.log)：真实 TLS、认证、隔离、目录/发现、无副作用安装夹具、日志、去重、重启与身份变化。未安装真实软件。 |
| 5 个模拟浏览器套件 | 首轮 **74/76**，2 个失败 | [原始日志](artifacts/browser.log)；地图 19、节点属性 15、发现 3 场景全部通过。旧“规划中”测试更新后 **2/2**：[复测](artifacts/manual-rerun.log)。网络协议编辑单场景复测通过，见 V01。 |
| 真实资产/实例绑定 API | 校准测试后 **21/21** | [首轮](artifacts/assets-api.log)、[最终](artifacts/assets-api-final.log)。测试保留全部实例但验证不可选原因，DELETE 按新 JSON 契约检查公开同步状态。 |
| 真实资产 CRUD 浏览器 | **通过** | [最终日志](artifacts/assets-browser-rerun.log)，覆盖管理员、网关、类型、地址预览、服务、离线节点、删除及桌面/手机。 |
| 真实绑定与权限浏览器 | **6 个子场景通过 / 1 个失败** | [最终日志](artifacts/binding-browser-final.log)。报告总计 8 tests / 6 pass / 2 fail 包含父测试失败，实际功能失败为 F06 一项。 |
| 未保存草稿与全局空闲探测 | **复现 F04/F05** | [日志](artifacts/ui-gaps.log)、[复现源码](ui-gap-probe.mjs)。此探测以确认现存缺口为目标，成功运行不代表产品行为通过。 |

### V01 · 网络设置协议编辑有一次未解释的超时

完整浏览器首轮在 `dashboard.test.mjs` 的 settings 协议场景等候 `/mypath` 输入时超时。单独复测同一源代码通过（[日志](artifacts/settings-repro.log)）；创建协议与其他设置场景通过。当前只确定测试存在一次不稳定结果，尚不足以把它定性为已修复或确定业务缺陷；保留首轮失败日志，后续应加失败截图/trace 和慢响应条件定位。

### V02 · 原生网络库全套受运行环境限制，且测试清理发生二次 panic

全套编译成功，但 credential 网络测试在 `run_network_command` 创建子进程时返回 `NotFound`，清理又在 `NetNS::switch_ns` 打开命名空间时遇到 `PermissionDenied`，最终二次 panic 使整个进程 SIGABRT。其他未完成测试不能推定通过或失败。

单独运行 netlink `addr_test`，确认 `ip link add dummy type dummy` 返回 `Operation not permitted`（[日志](artifacts/native-repro.log)）；单独运行 Magic DNS 测试，确认 TUN 初始化返回 `NotFound`（[日志](artifacts/native-dns-repro.log)）。当前 `/dev/net/tun` 不存在，进程有效/边界 capabilities 均为 0；`ip` 在独立检查中存在，因此未把全套首个 `NotFound` 武断归因于系统没安装 iproute2。

本项需要有 TUN、网络命名空间和相应权限的隔离 CI 环境补跑；建议测试夹具先检查能力并清晰报错，避免清理异常掩盖第一处失败。未修改业务代码，也未申请修改宿主网络权限来强行通过。

### 已修正的测试过期问题

- 两个真实页面套件仍使用 `#password input`；改成现有 `input#password`。
- 自动化浏览器测试仍模拟占位页并检查“规划中”；更新为真实 context/devices 结构、脚本库/空状态和手机菜单路径，仍检查空闲无请求、主题及中英文。
- 实例候选测试仍假设后端过滤隐藏角色不匹配的记录；现实现保留全部记录并给出不可选原因，测试已相应检查原因，未放宽权限约束。
- 服务 DELETE 已返回 `deleted` 和 `publication_sync`；测试从旧 204 改为检查 200 及完整结果。
- 实例显示名使用配置 hostname，而非 network_name；夹具的两个同名项目仍检查不同稳定 ID。
- 真实绑定浏览器改为独立子测试，单项失败不阻断其余检查，最终 JSON 报告仍明确失败。

业务代码未修改。上述测试校准不掩盖 F06；测试报告保留原始失败和后续结果。

## 测试覆盖及交付边界

- 覆盖登录/注册模拟、管理员/租户隔离、节点元信息、实例来源与变更、网关/设备/服务 CRUD、公开映射的后端应用/读回、服务卡片/导航、地图缩放/选中/离线地理数据、OpenWrt HTTPS 模拟发现/审阅/覆盖/ADB 联动、执行器 journal/撤销/清理，以及真实 HTTP 协议接线。
- 当前原生二进制的隔离资产夹具来自**当前迁移后的数据库**；没有旧版二进制制品，因此没有运行旧库备份恢复的独立 `node-instance-binding-recovery.test.mjs`。Rust 的迁移/历史保留测试本轮实际通过，不与真实旧二进制升级混淆。
- 原有 `central-e2e.test.mjs` 依赖名为 rust 的 Docker 容器和真实 Core 网络环境；本轮没有创建该环境，也没有运行该套件。桌面 GUI、移动 SDK、OHOS/iOS/JNI 等独立平台构建不在本轮控制台和 Agent 检查结论内。
- 未做真实 OpenWrt ACL/证书/ubus、fnOS Docker 初始化、八种软件实装、Android 配对/控件/中文输入/旋转、WebRTC 首帧、TURN、公网映射连通、1/2/5/10 路并发和资源性能、实机升级回退；均不能据模拟通过验收。
- 未触发新 CI、发布镜像或部署。上轮构建触发受 GitHub API 网络限制，仍无本轮镜像成功证据。
- 临时真实 Web 服务已停止。原夹具停止程序读取 `/proc/<pid>/exe` 受容器权限限制，本轮通过核对该夹具完整命令行、二进制路径及测试数据库路径后，仅停止了对应测试进程。
- 构建仍有原有 Browserslist/pnpm 提示及大包提示，地图离线行政数据所在异步包约 7.3 MB；未更改依赖或数据精度来消除提示。
- 文档也需同步：自动化任务看板仍写“未执行任何测试”，R10 部分会话身份/无 SSE 说明已被后续实现更新，Agent 集成文档仍写自动化未实现且链接到已不存在的 planned 目录；应按当前代码更新，保留历史轮次证据。

## 后续闭环顺序

1. F01/F02/F03：先形成可复现安装的两个仓库与执行器制品，并用真实进程跑部署检查。
2. F04/F05/F06：统一手动触发规则、保护脚本草稿、补管理员节点属性接线。
3. F07/F08：补失败步骤定位及准确的能力检查。
4. 落实用户已确认的 G03 手动导入，明确 G01/G02 的首版接入范围，更新文档和持续集成测试入口。
5. 完成上述后再安排实机验收，按前述真实媒体、安装、网络与回退矩阵逐项记录。

### 用户本轮确认的服务导入要求

用户回复：**“提供‘确认导入’按钮，手动选择设备归属、访问地址和服务后入库。”** 此项已进入必须闭环范围；本轮仍为完整性审计，没有声称已实现导入。

建议实施验收点：

1. 从最近一次成功发现快照选择服务，失败来源或历史结果明确标记，不能假装当前在线。
2. 用户明确选择目标账号内的节点/设备、协议/端口、名称、访问地址和是否加入导航；不根据 IP/容器名自动猜归属。
3. 服务端重验 Agent 和资产账号权限、设备归属、端口冲突及访问地址；导入写现有 `asset_services`，不再建第三套服务表。
4. 用 Agent 身份/来源服务 ID 与导入幂等键防重复；再次导入显示差异，由用户选择更新/跳过。来源消失不自动删除人工资产。
5. 页面保存后直接使用返回的资产记录更新服务管理/导航；不增加轮询、后台发现或自动同步定时器。
6. 覆盖重复点击、任务重放、跨账号/删除设备、同端口冲突、部分导入失败、地址清空与导航开关的真实 HTTP 和浏览器回归。
