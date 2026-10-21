# ECO-102：页面与请求基线证据

记录日期：2026-10-21 UTC。负责人：`iteration_exec2`。关联 [ECO-102](../../../task/task-eco-102-page-baseline.md)、[需求](../../../require/README.md)。

## 采用基线与证据等级

- 实现基线：`1e92296b17fc2e85dcd4521b6012232e11abed95`，包含用户管理、密码能力判断及测试二进制定位修复。
- 文档基线：`670eb3b42800d42333356da0cb15f41d76c92f1e`；总控整合到 `feat/ecosystem-web-ui`，工作区 `/workspace/EasyTier-iteration`。
- 以下源码事实来自上述实现基线。本记录不将源码推断、历史测试结果或模拟接口当作本次真实页面验证。
- 本任务不修改业务源码，不新建生态接口，不改变既有查询周期、账号权限或 config-server 行为。

## 入口与身份边界（源码已核实）

`easytier-web/frontend/src/main.ts` 使用 hash 路由；以下路径应在浏览器 URL 的 `#` 后使用。

| 路径 | 现有入口与边界 |
| --- | --- |
| `/auth`、`/auth/login`、`/auth/register` | `Login.vue`，登录/注册复用同一组件；可用方式由真实认证策略决定 |
| `/h`、`/h/dashboard` | `MainPage.vue` 壳层与 `Dashboard.vue` |
| `/h/users` | 用户管理，`MainPage.canManage` 检查 `users.manage` 且非强制改密；服务端仍负责授权 |
| `/h/password` | 本地改密；菜单仅 `local_password_enabled === true` 可见，`ChangePassword.vue` 接收能力与来源 |
| `/h/deviceList` | 设备宿主列表及接入命令；此处 device 不是生态下挂业务设备 |
| `/h/deviceList/device/:deviceId/:instanceId?` | 宿主/实例管理抽屉路径，不能用 peer ID 替换稳定宿主身份 |
| `/h/networks`、`/h/networks/:networkId` | 中央网络列表/详情；外部 Console 模式由壳层返回仪表盘 |

`MainPage.refreshIdentity` 获取当前身份，强制改密时跳转改密并清除临时凭据；401 结束会话。生态入口必须保留这些边界，不能仅因加入菜单而获得权限。

## 请求来源（静态事实，运行频率另行取证）

API 路径相对 `/api/v1`；请求封装位于 `frontend/src/modules/api.ts` 和 `userManagement.ts`。

| 所有者/入口 | 请求与触发 | 生命周期与复用限制 |
| --- | --- | --- |
| `MainPage.loadNavigation` | 挂载即执行、之后 `setInterval(..., 10_000)`；身份 `/auth/me`，首次 `/console-info`，中央模式 `/networks` 与 `/blocked-devices` | 卸载清理 interval。新增生态资源查询不能接入该定时器 |
| `DeviceList.loadDevices` | `/machines`；`Utils.PeriodicTask(..., 1000)` 启动时立即查询，异步任务结束后递归 timeout；另有手动刷新 | 不是固定 1 秒 interval；卸载 stop。直接嵌入整个列表会携带旧周期副作用 |
| `DeviceList` 挂载 | `/console-info`，供空态接入命令使用 | 不应把查询失败解释成未登记或离线 |
| `NetworkDetail.openNodeDetail/loadNodeDetail` | 打开节点后查询 routes、peers、ACL，随后 5 秒 interval | 通过 `/machines/:machine_id/proxy-rpc` 调用 `list_route`、`list_peer`、`get_acl_stats`；关闭详情清理定时器。不得直接作为新按需面板的生命周期 |
| `MainPage` 用户页面 | 路由/权限变化、显式动作后读取用户和事件 | 有请求序号与临时凭据清理；不要复制会话所有权到生态组件 |

`NetworkDetail` 的 routes/peers/ACL 使用 `Promise.allSettled`；失败可保留已有结果，不能据此推导节点离线或资源健康。以上请求清单只覆盖本任务读到的入口，不宣称全仓无其他周期请求。

## config-server 兼容基线

接入命令格式保持 `easytier-core --config-server ${proto}://${host}:${port}/${username}`，协议、端口、用户名来自 `/console-info`。`MainPage` 的 host 来自 `new URL(getApiBase(), window.location.href).hostname`；`DeviceList` 来自 `window.location.hostname`。两者当前来源不同，跨源部署下不能未经契约核实就合并为同一实现。本任务只记录，不执行命令、不接入真实设备、不修改 VPN/路由/防火墙。

## 页面取证环境与当前限制

- 新工作区最初没有依赖或构建产物；旧工作区的 dist/二进制不能冒充当前页面产物。
- 已将既有 pnpm 依赖复制到新工作区，当前共享库完成 protobuf 生成、`vue-tsc -b` 与 Vite 构建；生成源码无差异。旧工作区产物未修改。
- 原 `pnpm ... build` 入口触发自动依赖检查，因 `/home/agent/.local/share/pnpm` 不存在失败；使用清单中相同工具逐项执行。protoc 自动下载解析失败后使用既有本地归档，不新增下载。
- `/usr/bin/chromium` 可用；未复用残留浏览器用户数据或会话。
- 旧二进制对应后端源码与本基线比较无差异，但不能据此声称重新编译验证。后续如复用，仅用于真实 API；页面必须来自本工作区当前源码。
- 服务启动前发现 `main.rs:get_config_server_listeners` 固定通配监听，缺少单独 loopback 地址参数；临时端口 0 不等于 loopback 绑定。已向总控报告，待明确隔离执行方式后再启动，不自行改变后端。

## 尚未验证及下游交接

| 项目 | 当前证据/缺失 | 对 ECO-103 的影响 |
| --- | --- | --- |
| 登录、权限、设备空态、网络页面真实渲染 | 待隔离服务启动和浏览器采证 | 不以源码清单声称视觉或运行行为通过 |
| 实际请求顺序和空闲频率 | 静态触发已核实，网络记录未采集 | 按需契约必须显式排除旧 timer 副作用 |
| 真实宿主、所选成员、节点 routes/peers/ACL | 没有隔离的已连接设备/实例 | 仅确认接口和入口，不能宣称真实拓扑、直连/中继、ACL 有效 |
| 业务资源与公网/导航入口 | 未提供真实来源/API/公网环境 | 交 ECO-101 能力清单；缺失能力显示未支持，不编造资源成功状态 |
| config-server 实际连接/认证/下发 | 仅静态命令与身份来源核查 | 保持既有语义，后续集成必须区分测试设备与真实网络验证 |

未取得的证据不阻塞已核实静态契约，但禁止下游将其作为已验证能力。截图、浏览器动作与网络摘要取得后在本目录追加，并记录基线、端口、虚构数据和退出清理。
