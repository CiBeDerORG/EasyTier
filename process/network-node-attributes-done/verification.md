# 网络节点属性：验收与部署

2026-10-22，工作树 `/workspace/EasyTier-main-node-attributes`，分支 `feat/node-attributes-main`，基线 `origin/main@e307dadb`。本次改动尚未提交；已部署的是当前工作树的构建产物。

## 已执行的验证

先 `source /workspace/.easytier-env/activate.sh`，使用工作区已有 Rust/Protobuf 工具链。

|检查|命令/证据|结果|
|---|---|---|
|后端|`cargo test --locked -p easytier-web`|294 通过、0 失败、2 原有大规模连接用例跳过；包含迁移、归属、原子更新、严格字段校验、TCP/UDP IPv4/IPv6 连接与取消后的写入顺序|
|前端构建|`pnpm --dir easytier-web/frontend build`|通过；沿用项目已有 bundle 大小与 Browserslist 提示|
|数据映射|`pnpm --dir easytier-web/frontend-lib exec vitest run tests/node-attributes.spec.ts`|2 通过，覆盖旧响应与离线属性|
|浏览器交互|frontend 目录下 `WEB_TEST_CHROMIUM=/usr/bin/chromium node --test tests/node-attributes.test.mjs`|首轮9通过；移动端测试定位器歧义，修正后以 `--test-name-pattern='mobile list'` 定向复测1通过，共10个场景通过|
|Web 部署构建|`cargo build --locked -p easytier-web --features embed`|在前端构建后执行，通过；含本次静态页面|
|演示客户端构建|`cargo build --locked -p easytier --bin easytier-core`|通过；客户端源码与 main 一致|
|真实浏览器联调|`node /workspace/.easytier-node-attributes-preview/verify-browser.mjs`|UI 登录、既有表格/卡片切换、取消无请求、保存/trim/重载/清空、只读 IP、手机宽度及浏览器无运行时异常通过|
|实际重启/重连|`reconnect-result.json`|服务端重启、客户端重连后手填两字段及别名不变，IP 观察时间更新，已登录列表显示在线|
|独立只读审核|后端字段边界、权限、迁移、config-server 身份与协议路径|未发现需要修正的问题|

原有跳过用例分别需要 4097 个 TCP 连接、1000 个真实管理连接及提高文件句柄上限。自动 IP 检查使用本地 MMDB；没有外部 GeoIP 服务或新增周期任务。

原登录、别名、删除、设备管理、搜索、刷新和视图选择保留。新增 UI 为单项“编辑属性”入口、地点/型号摘要和两字段编辑弹窗。手填地点独立于 IP 推测所在地；自动字段只读。

## 当前云环境部署

- Web/API：`http://127.0.0.1:11221`，监听 `0.0.0.0:11221`。这是当前云环境的地址，未配置可从公网访问的域名或端口映射。
- config-server：TCP `22030`；演示客户端连接 `tcp://127.0.0.1:22030/admin`。
- 独立目录：`/workspace/.easytier-node-attributes-preview`；包含本次 Web/Core 二进制、`preview.db`、日志、PID 文件、管理脚本和截图。
- 账号信息：该目录下 `credentials.json`，权限 `0600`。默认管理员密码已更换为随机密码，不写入源码或本文。
- 查看状态：`python3 /workspace/.easytier-node-attributes-preview/manage.py status`。
- 停止本实例：`python3 /workspace/.easytier-node-attributes-preview/manage.py stop`。
- 启动本实例：`python3 /workspace/.easytier-node-attributes-preview/manage.py start`。

管理脚本仅操作记录且二进制路径匹配的本实例 PID。没有写入系统启动项。运行依赖本云环境存活；本次没有修改其他服务或数据库。

演示节点使用实际 Linux EasyTier 客户端，别名为“OpenWrt 演示节点”，并非已经接入用户家中的 OpenWrt。地点和型号是标明“演示”的手填样例。来源 IP 为真实观察到的 `127.0.0.1`，界面明确显示“非公网 IP，无法定位”。用户实际 OpenWrt 经 NAT 接入时记录的是服务器观察到的出口 IP。

## 截图和日志

- [设备卡片](/workspace/.easytier-node-attributes-preview/screenshots/device-card.png)
- [编辑弹窗](/workspace/.easytier-node-attributes-preview/screenshots/edit-attributes.png)
- [手机编辑弹窗](/workspace/.easytier-node-attributes-preview/screenshots/mobile-edit.png)
- [删除旁齿轮：原设备网络管理抽屉](/workspace/.easytier-node-attributes-preview/screenshots/device-settings.png)（按用户追加请求打开并截图；演示节点暂无网络实例）
- [真实浏览器结果](/workspace/.easytier-node-attributes-preview/browser-result.json)
- [重启/重连结果](/workspace/.easytier-node-attributes-preview/reconnect-result.json)
- 构建与测试日志已留在部署目录的 `verification-logs/`。

截图来自已部署程序及真实数据库，未使用接口夹具。原列表布局与按钮保留；编辑弹窗在 390px 手机上无横向溢出。

## 数据与迁移边界

内置 GeoIP 为 Country 库，区域/城市可空。若需要城市精度，可通过原有 `--geoip-db` / `ET_GEOIP_DB` 指定兼容的本地 City MMDB；本次没有下载城市库。

迁移为 `m20261022_000011_node_attributes`，在 main 的 000010 后增加八列。旧记录按 nullable/`unknown` 初始化，不伪造历史公网地址。正式升级前备份数据库；若需要回退，不能直接把不认识新迁移的旧二进制指向该库。应使用迁移工具同步回退 schema 与迁移账本，或使用保留迁移识别能力的回退版本。显式 down 会丢弃本次八列数据，其余原节点数据保留。

后续只改本需求的文档时，仅核对内容和链接，不重跑测试、构建或 CI。
