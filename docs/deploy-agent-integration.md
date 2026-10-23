# deploy Agent 管理入口

第一版入口：登录 EasyTier Web，选择侧栏「应用与服务」，路由 `/#/h/applications`。设备运行 CiBeDerORG/deploy 中本次新增的 Go Agent；安装方式见 deploy 仓库的 `agent/README.md`。

## 操作模型

- 页面挂载、切换设备、重新读取已保存数据只查询 Web 数据库。
- 连接验证、服务发现、读取设备应用目录、安装/更新、任务状态和日志读取均需显式点击。
- Web 每次操作建立一次经证书校验的 HTTPS 连接，关闭 keepalive、连接池复用、HTTP/2、自动重试和重定向。不使用配置服务的心跳/RPC 通道。
- Agent 为被动监听端；不存在 Agent 向 Web 注册的常驻反向连接。需要 Web 能直接访问的 HTTPS 地址。经过 EasyTier 虚拟网络访问时，原有网络自身的心跳仍存在。
- 页面只显示最后查询时间与快照。查询失败保留数据；服务来源部分失败时保留该来源的历史行并标记为历史结果。
- `MainPage.vue` 原有导航定时刷新已改成刷新按钮。其他现有网络页面/内核的周期行为不属于本次 Agent 管理链路。

## 服务端部署

正常构建前端与 Web，不需要新的外部服务：

```sh
pnpm install --frozen-lockfile
pnpm --dir easytier-web/frontend build
cargo build -p easytier-web --release --features embed
```

首次启动自动运行数据库迁移，新增 `deploy_agents` 和 `deploy_tasks`。设备与任务归属于登录用户，禁止跨用户读取或操作。已有加密记录但主密钥缺失时拒绝启动，避免静默生成另一把密钥；需恢复原密钥。

服务端在数据库同目录生成 `<数据库文件>.deploy-agent.key`（32 字节，Unix 权限 0600），AES-256-GCM 加密保存设备凭证和含敏感参数的任务请求。恢复时须同时恢复数据库与密钥。Docker 挂载整个数据库目录，不能只持久化数据库文件。启动服务的用户需要该目录写权限。

设备 token 与 EasyTier config-server URL 的用户名无关，不做密码散列、裁剪或替换；传给 Agent 时使用 `Authorization: DeployToken <base64 UTF-8 原值>`。设备绑定 `agent_id` 后，每次请求与响应都校验身份。自签名证书须填入添加设备表单的信任证书字段，证书 SAN 必须匹配访问主机名/IP。

没有自动证书续期、任务清理、依赖安装或 Agent 更新。设备源码包更新由管理员执行，再点击读取应用目录；任务绑定源码内容 revision，避免旧表单调用新脚本。

## HTTP 接口

所有接口沿用 Web 登录会话。以下 `id` 是 Web 连接记录的 UUID，`task` 是任务 UUID。访问 Agent 的动作使用 POST，GET 不触发设备请求。

| 方法与路径（前缀 `/api/v1/deploy`） | 行为 |
| --- | --- |
| `GET /agents` | 已保存设备、身份、发现快照和应用目录；不返回凭证 |
| `POST /agents` | 保存 `{alias, endpoint, ca_pem, token}`，不连接设备 |
| `DELETE /agents/{id}` | 删除连接与 Web 任务记录；不卸载设备软件 |
| `POST /agents/{id}/probe` | 校验证书、凭证、协议版本并绑定设备身份 |
| `POST /agents/{id}/discover` | 发现 Docker/Compose/systemd/procd 服务 |
| `POST /agents/{id}/catalog` | 读取该设备部署包的应用目录 |
| `GET /agents/{id}/tasks` | Web 保存的任务状态与已读取日志 |
| `POST /agents/{id}/tasks` | 提交安装或更新任务 |
| `POST /agents/{id}/tasks/{task}/refresh` | 查询原任务状态 |
| `POST /agents/{id}/tasks/{task}/fetch-logs` | 按保存的游标读取下一段日志 |
| `POST /agents/{id}/tasks/{task}/resubmit` | 解密原请求，使用同一任务 ID 显式重发 |

安装请求示例：

```json
{
  "task_id": "8d81c0b4-a72d-4576-9cb9-4fffd5bc1ff1",
  "application_id": "portainer",
  "action": "install",
  "variant": "docker-compose",
  "catalog_revision": "从该设备目录响应获取的 revision",
  "parameters": { "PORTAINER_HTTP_PORT": "7004" },
  "secrets": {}
}
```

服务端补入已绑定的 `agent_id`，先持久化加密请求和 `outcome_unknown` 记录，再向 Agent 发送一次。相同 ID、不同内容冲突；连接失败不会自动重发。Agent 收到请求后先保存回执再启动执行器，同 ID、同内容返回原结果。

状态包括 `accepted`、`running`、`succeeded`、`failed`、`interrupted`、`outcome_unknown`。页面的状态始终是上次查询值。网络错误或不确定是否已送达时，点击查询原任务或按原编号重发；不能据此推断安装没有执行。Agent 重启不恢复旧安装，只把未结束记录标为中断。

一台设备同时仅允许一个修改任务，忙时拒绝、不排队。Agent 与 deploy CLI 共用文件锁，两个配置文件分别原子写入并保留修改前副本，不承诺安装脚本整体事务回滚。

## 当前边界

容器安装需要已有 Docker/Compose；纯净 fnOS 的官方 Docker 应用初始化仍待针对实际版本适配。OpenWrt 当前只开放现有 EasyTier 原生安装，ARM64 和 ARMv7 分别构建。fnOS 应用中心元数据、自动资源归属映射、任务取消、通用卸载和应用实际镜像 digest 记录不在第一版中。

不新增管理心跳或定时器。安装的应用本身、已有安装脚本和现有 EasyTier 网络仍可能有 healthcheck、有限等待、重试或协议心跳，不能把它们描述成整个系统都无计时行为。

## 验证与复现

```sh
# 服务端隔离、加密、未知结果保留、部分来源失败
cargo test -p easytier-web deploy_agent::

# 真实浏览器：页面空闲与提交后无轮询，按钮才发起设备操作
pnpm --dir easytier-web/frontend test:agent

# 构建真实二进制后，跨仓库运行 HTTPS 和安装链路测试
bash /path/to/deploy/agent/build.sh
cargo build -p easytier-web --features embed
cd easytier-web/frontend
DEPLOY_REPO=/path/to/deploy node tests/deploy-agent-integration.mjs
```

浏览器测试需要 Playwright Chromium 或系统 Chromium，可用 `CHROMIUM_PATH` 指定。`WEB_SCREENSHOT_DIR` 可保存桌面和移动端截图。

联调需要 Linux 和 Node 22+，可用 `DEPLOY_AGENT_BINARY`、`EASYTIER_WEB_BINARY` 指定二进制。测试创建临时数据库和部署源，通过真实 `deploy.sh` 配置初始化、同步和 Agent 执行器运行无副作用的安装夹具；Docker/所有权操作在夹具 PATH 中替身处理，不安装真实应用。覆盖 TLS 信任、错误凭证、用户隔离、发现、日志、重复提交、进程重启、身份替换与持久化密钥。

交叉编译和夹具验证不代表已在 fnOS/OpenWrt 实机完成八个应用的安装验收。
