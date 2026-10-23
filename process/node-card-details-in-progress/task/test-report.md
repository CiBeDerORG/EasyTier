# 网络节点卡片详情：测试报告

## 2026-10-22：网络节点命名与网络拓扑菜单

状态：源码已修改，调试模式；本轮测试、类型检查、lint、构建及 Actions/CI 均**未执行**，未重新部署。

改动范围：

- `frontend-lib/src/locales/cn.yaml`、`en.yaml`：菜单、页面标题及仪表盘对应区域统一显示“网络节点 / Network Nodes”；三个明确引用列表的辅助提示同步命名。新增“网络拓扑 / Network Topology”菜单文案。
- `frontend/src/components/MainPage.vue`：共用工作区菜单顺序为“仪表盘 → 网络拓扑 → 网络节点”，覆盖桌面侧栏及手机抽屉；现有标题、高亮计算自动使用新菜单项。
- `frontend/src/main.ts`、`components/NetworkTopology.vue`：新增 `/h/topology`（路由名 `networkTopology`）与空白页面，不实现拓扑内容、不新增接口请求或周期任务。

保留已有 `deviceList` 路由名、路径和 API，当前卡片字段、属性编辑、设置和数量等实现继续保留。现有 `11221` 仍为前次部署，未包含本轮命名/菜单及上一轮头部样式修正；旧截图与13项通过记录不能作为这些新增修改的验证结果。

待测项：中英文菜单与页面标题一致；仪表盘下面紧邻网络拓扑；点击拓扑时路由/高亮正确且正文为空；切回网络节点保留原列表操作；手机菜单同步。当前尚无本轮运行时验证结论。

## 2026-10-22：追加恢复地区图标、版本胶囊和数量徽标

状态：源码已修改，未验证、未重新部署；继续调试模式。上一轮“测试 我看看页面”的授权不自动扩展到本轮修改。

改动仅涉及 `frontend/src/components/DeviceList.vue` 的桌面卡片头部：

- 地区前恢复 `pi-map-marker` 定位图标，地区与操作按钮同排。
- 标题右侧版本恢复蓝底白字胶囊，使用原版内边距 `.1rem .4rem`、圆角 `.75rem`、字号 `.65rem`；悬停可读完整版本，详情中的完整版本继续展示。
- 将左侧“0 个实例”文字恢复为设置齿轮左侧的数量徽标，读取现有 `running_network_count`，浅蓝背景、深蓝文字，通常 24px 圆形；0 也显示。
- 编辑属性仍紧邻设置齿轮右侧；别名、删除、六项详情和手填地点/型号保留。表格和手机样式未改。

原版样式依据：中央布局调整前 `a017dc13` 的 `DeviceList.vue` 中 `.version-badge`、`pi-map-marker location-icon` 及 `w-6 h-6 bg-blue-100 text-blue-800 rounded-full`。本轮固定版本胶囊为截图的蓝色，避免当前主题主色改变它。

测试、类型检查、lint、构建、Actions/CI：**未执行**。没有新增测试文件或触发提交/push。端口 `11221` 仍为上轮通过13项浏览器用例的版本，旧结果和旧截图不代表这次头部调整。

待测：地区图标和长地区名称布局；蓝色版本胶囊及完整版本提示；0/1/多位网络数徽标紧邻齿轮；编辑属性与原设置等按钮仍可点击。尚无本轮浏览器验证结论。

## 2026-10-22：用户授权测试并查看页面

用户本轮明确要求“测试 我看看页面”。仅执行本次页面变更的验证和部署所需构建，调试模式仍然有效；没有运行后端全量测试、lint、Actions/CI 或 Git 提交/推送。

|执行项|命令与结果|
|---|---|
|前端类型与产物|frontend 目录下 `pnpm exec vue-tsc -b && pnpm exec vite build`，通过；复用未变化的 frontend-lib 已构建产物|
|专项浏览器|`WEB_TEST_CHROMIUM=/usr/bin/chromium node --test tests/node-attributes.test.mjs`，13 项通过，0 失败；原10项与新增3项一起验证|
|嵌入 Web 构建|`cargo build --locked -p easytier-web --features embed`，通过；用于把本次页面部署至原端口|
|部署核对|新服务返回的 JS 资源路径与本次 frontend/dist 一致，继续使用原独立演示数据库和客户端|
|真实页面|`node /workspace/.easytier-node-attributes-preview/verify-card-details.mjs`，通过；实际登录，六项字段可见，连接 URL、完整版本和 UUID 与真实 API 一致；编辑按钮与齿轮同排且位于详情之上；编辑弹窗读取原有地点/型号，取消不写入；原齿轮仍打开网络实例管理；无浏览器运行时异常|

新增3项浏览器用例覆盖六项字段完整常显、按钮相邻关系、在线/离线的6种时间回退组合，以及长 IPv6、UUID、版本换行而不溢出或截断。既有用例覆盖属性保存/取消/失败保留输入、默认表格展开及手机编辑交互。

页面已更新到当前云环境 `http://127.0.0.1:11221`。本次截图来自真实运行后端，未用接口夹具代替：

- [卡片近景](/workspace/.easytier-node-attributes-preview/screenshots/card-details-closeup.png)
- [完整页面](/workspace/.easytier-node-attributes-preview/screenshots/card-details-updated.png)
- [新位置打开属性弹窗](/workspace/.easytier-node-attributes-preview/screenshots/card-details-editor.png)
- [实际部署验证结果](/workspace/.easytier-node-attributes-preview/card-details-result.json)

日志位于 `/workspace/.easytier-node-attributes-preview/verification-logs/node-card-details-*.log`。本轮无未解决测试失败；已有 bundle 大小提示保留。演示地点/型号仍明确标注演示值，用户实际 OpenWrt 公网接入不在本轮页面验证范围。

## 上一轮记录：2026-10-22 恢复卡片字段与调整编辑入口

当时状态：源码修改已完成，调试模式；未验证、未重新构建部署。后续授权结果见上节。

本轮源码改动范围：

- `easytier-web/frontend/src/components/DeviceList.vue`：将编辑属性按钮移到卡片上方原齿轮旁，恢复卡片常显详情。
- `easytier-web/frontend/src/components/DeviceDetails.vue`：支持卡片展示 Hostname、Public IP、Network Count、Last Seen、Version、Machine ID 六项。Public IP 使用原连接 URL，保留协议和端口；Last Seen 使用 `last_seen`，缺失时回退 `report_time`。

执行结果：本轮处于调试模式，测试、构建、lint、CI 均**未执行**。尚未重新构建或部署，端口 `11221` 仍运行前一版；旧测试结果和旧截图不代表本轮改动已经验证。

待测项：

- 六项字段完整展示，覆盖在线、离线及时间缺失情况，确认 Last Seen 回退行为。
- 长 Public IP/连接 URL 和 Machine ID（UUID）正常换行，不挤压卡片布局。
- 卡片顶部编辑属性按钮的位置及交互正确，原设置、别名编辑、删除行为保持可用。
- 表格和手机视图的默认详情展示与原操作保持不变。

已知问题：本轮尚无验证结论；运行页面未更新，不能用于验收当前源码。阶段性完成后，依据上述清单一次性提示用户是否授权测试。
