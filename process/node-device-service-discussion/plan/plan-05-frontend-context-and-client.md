---
id: PLAN-NDS-05
status: verified
updated_at: 2026-10-23
tasks: [T08, T09]
---

# P05 页面上下文与接口客户端

## 当前状态（2026-10-23）

状态：**已验证（`verified`）**。

T08–T09 已通过前端类型检查及构建，并完成真实接口和页面 E2E：管理员账号/节点选择、显式查询、新设备被筛选排除后的清空及定位展开、多设备展开和离线节点资料管理均在报告范围内。

验证依据：[2026-10-23 验证摘要、日志和截图](../task/verification-2026-10-23.md)。已验证仅指报告列出的实际执行范围，不表示下方规划的完整场景矩阵全部测试完成。

本次文档同步未运行测试、构建或 CI；未来验证仍遵守调试模式和用户授权范围。

## 规划与联调前交接记录（历史）

> 以下保留最初计划、代码基线和集中验证前的实施交接。原文的“本轮未执行”“待授权验证”“尚未实施”等描述仅记录当时阶段；当前进度和实际结果以上方状态及验证摘要为准，不能将旧基线当作现在的代码状态。

本计划承接[统一范围](../require/require-10-consolidated-scope.md)、[接口与工程默认](plan-01-baseline-and-contracts.md)及[前端答复](../require/require-09-frontend-grouped-decisions.md)。目标是在原左菜单、右功能框架中提供独立的设备与服务资料页面，明确操作者、资料所属账号及所选节点；新页面不复用旧运行实例抽屉。

### 代码基线与差距

|实际代码|现状|本计划处理|
|---|---|---|
|`easytier-web/frontend/src/modules/api.ts`：`ApiClient`、`ConsoleInfo`|axios 实例为 private，已有 cookie 和 401 处理；只有旧机器接口，没有新资产 DTO/API，ConsoleInfo 没有完整管理员能力|T08 扩展现有客户端，按 T01 契约增加 `/api/v1/assets` 方法与类型，不另建绕过认证的客户端|
|`frontend/src/main.ts`：`deviceList` 下的 `deviceManagement`；`DeviceList.vue`：`selectedDeviceId`|旧 `deviceId` 路由参数会打开实例 Drawer|T09 交付独立 `NodeAssetsPage.vue`；路由及菜单由 T14 接入，不占用旧参数|
|`MainPage.vue`：`centralEnabled`、`loadNavigation`|前者只是部署模式判断，后者已有定时刷新|新管理员能力来自服务端；新资产查询不接入旧轮询|
|`DeviceList.vue`：`search`、`expandedRows`、`handleDeviceManagement`|旧列表查询为本地过滤，行展开显示注册节点字段|新页独立管理已应用筛选、分页、设备 ID 展开状态及按需服务加载|

### 交付范围

- T08 固定前后端 DTO、错误分类及组件 Props/Events，复用现有 ApiClient 的认证传输；其后 T09–T12 才可按契约并行。
- T09 管理管理员先账号后节点、普通账号自身范围，节点必选；有效显式链接优先，否则恢复当前登录身份上次仍有权限的节点，首次或失效时待选。
- 刷新/重新打开详情只恢复节点，组、类型、关键词重置；账号和节点变更隔离请求与缓存，不将旧资料显示在新上下文中。
- 组/类型/关键词点击“查询”后生效；组筛选仅全部设备或某组。翻页与 CRUD 后刷新使用已应用条件，不偷用尚未提交的筛选草稿。
- 多设备服务可同时展开，按展开、操作完成或手动刷新读取；不新增长连接、心跳、轮询或定时任务。
- 新增设备成功后定位展开；被已应用筛选排除时自动清空组/类型/关键词再显示，节点不变。回原节点列表按 F15 回初始查询/页码/位置，保留既有视图/语言/主题偏好。
- 节点 `is_public_server` 仅作手工静态属性，在节点上下文显示及维护；不据此开启公网发布或判断实际可达。

### 任务与独占范围

|任务|唯一写入者范围|交接|
|---|---|---|
|[T08 前端类型与接口客户端](../task/task-08-frontend-api-contract.md)|新 `types/node-assets.ts`、既有 `modules/api.ts`|DTO、接口方法、组件类型契约、错误及重试规则给 T09–T12|
|[T09 详情页上下文与查询](../task/task-09-node-assets-page.md)|新 `components/NodeAssetsPage.vue`、`composables/useNodeAssetsPage.ts`|页面导出、子组件接入点、路由定位需求、翻译 key 清单给 T14|

`main.ts`、`MainPage.vue`、`DeviceList.vue`、旧 `DeviceAttributesDialog.vue`、`console.css` 和中英 YAML 均由 T14 唯一接入。T08/T09 不借调这些共享文件；未完成的组件连接采用契约中的 Props/Events/slot 交接，不要求并行 agent 改 Page 文件。

### 验收与边界

验收场景由 T15 在获得测试授权后执行，本轮未执行：管理员账号隔离、显式节点/记忆节点选择、失效节点处理、查询按钮、分页、晚到响应隔离、多设备展开、离线和零设备、保存后定位、返回原列表以及 401/403/404/业务冲突。模拟接口只能用于组件开发，不能代表真实接口已联通。

部署模式不支持时，新资产操作不可提交，既有 external webhook 功能保持；不修改 OIDC/config-server。F16–F18 未答撤回，类型工程默认以 P01 明示，不将未答选项伪装为确认。具体路由拼写、ID 类型、错误码及静态节点属性接口由 T01 冻结，不能由各组件各自发明。

### 2026-10-23 实施交接

对应代码已写入并接入，具体路径和交接内容见任务看板。本轮未运行测试、构建、类型检查、CI 或部署；验收标准未标为通过，运行验证由 T15 按授权执行。
