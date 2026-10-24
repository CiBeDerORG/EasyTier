# EasyTier 手机执行器

本目录是独立手机执行器源码与部署材料。主服务镜像不包含执行器。`feat/automation-center` 推送执行器相关变更会独立构建并在冒烟检查通过后发布 `linux/amd64` 镜像到 `ghcr.io/cibederorg/easytier-automation-executor:feat-automation-center`；也提供 `sha-<完整提交号>` 固定版本标签。镜像构建及真机验收结果以对应 Actions 为准，不将配置文件存在视为已部署。历史验证见 [记录](../process/openwrt-discovery-sync-in-progress/task/test-report.md)。

## 部署位置与配置

首版在Linux x86_64/NAS运行一个执行器，使用现有EasyTier可达的手机IPv4；不扫描设备、不周期重连，不导入第二套资产；用户触发的服务端点变化由主服务发起一次清理和连接。镜像使用Google Linux platform-tools，尚未提供ARM镜像验证。OpenWrt承担网络与发现职责。

EasyTier 主服务现在支持直接在「自动化中心 → 设置」中配置执行器，入口仅对管理员显示：

1. 部署执行器并配置下述 Token、回调地址和监听地址。
2. 在设置页填写 EasyTier 服务端可访问的 HTTP/HTTPS 基础地址及相同 Token，点击「保存设置」。配置立即生效，服务重启后仍保留；无需再往 EasyTier 容器中注入执行器变量。
3. 点击「检查已保存连接」验证认证和协议身份。读取设置及普通保存不做探测，不增加周期检查。检查成功仅表示接口可达，不能证明 ADB、媒体依赖或真实手机画面已就绪。
4. Token 可显式保留或替换，读取设置不会回显原值。数据库中的凭证使用主服务现有 AES-GCM 密钥加密，备份时必须保留数据库旁的 `.deploy-agent.key`。

修改配置前先关闭画面并完成/取消任务，确认资源清理；正在派发的操作也会阻止修改。切换地址会先通过旧地址释放已记录的空闲连接，手机需重新连接；清理未确认时保留原配置。不同管理员并发编辑由版本号检查阻止覆盖。

旧部署仍可使用以下 Rust 后端环境变量，作为尚未通过页面保存时的兼容来源。页面保存后以数据库配置为准：

```text
EASYTIER_AUTOMATION_RUNNER_URL=http://127.0.0.1:8766
EASYTIER_AUTOMATION_RUNNER_TOKEN=<与执行器相同的32至4096字节无控制字符秘密>
```

执行器复制.env.example为本机.env，填写同一私有token和真实AUTOMATION_CALLBACK_URL。示例11211端口需按实际后端监听修改，不代表现有服务必定使用此端口。回调地址应直接到受信任主服务，可经EasyTier；不会跟随重定向或继承外部HTTP代理。

token示例故意留空，必须填写随机私有值后才能启动。视频会话与脚本任务均绑定 journal 的稳定 executor_id；创建、关闭及清理确认核对身份。更换 URL 或数据卷不能代替旧执行器清理。升级前关闭旧版活动会话；没有身份记录的历史会话保留 cleanup_pending，需实机确认旧执行器已停止，不会用新执行器的响应冒充旧资源释放。

凭证传输保留原值：普通可打印 ASCII Token 继续使用原有 Bearer；含空格或非 ASCII 字符的 Token 使用 `AutomationToken <UTF-8 Base64>`，防止 HTTP 头裁剪空白或改变编码。此类 Token 需要主服务和执行器同时更新；两端均兼容原有 Bearer 认证。Base64 仅是传输编码，连接保护仍取决于 HTTP/HTTPS 和部署网络。

```sh
cd automation-executor
cp .env.example .env
# 填入 AUTOMATION_TOKEN（至少 32 字节）、实际 AUTOMATION_CALLBACK_URL；
# 跨主机访问时明确设置 AUTOMATION_BIND_IP 为可达的受信任私网监听地址。
docker compose pull
docker compose up -d
```

独立镜像由 [Automation Executor Image](../.github/workflows/automation-executor-image.yml) 在执行器源码或自身工作流变化时构建发布，不需要重新编译 Rust 主服务。仅本地开发需要源码重建时，可以在此目录执行 `docker build -t ghcr.io/cibederorg/easytier-automation-executor:feat-automation-center .` 后再执行 `docker compose up -d --pull never`。以上部署操作还需在实际主机验收。启动时用文件锁拒绝第二个管理进程；不得设置多个uvicorn workers或让多个容器共用该数据卷运行。生产站点浏览器只访问Rust入口，执行器HTTP只开放可信私网接口，不对浏览器发放其token。ADB server的5037端口不对外发布。

自动化页面与Rust API目前要求同源。反向代理须保留Host并转发WebSocket Upgrade；Rust与Runner需私网互通，AUTOMATION_CALLBACK_URL必须从Runner可达Rust。信令HTTP连通不代表WebRTC ICE媒体路径已经可达。

## 网络与媒体

- compose使用Linux host网络，以避免将容器内部ICE地址误发给浏览器。媒体UDP端口由ICE分配，实际防火墙和可达性仍需验证；开放网页HTTP不等于视频能连接。
- 默认未配置STUN/TURN。AUTOMATION_ICE_SERVERS接收标准ICE配置JSON数组。
- 公网需要中继时，配置AUTOMATION_TURN_URLS JSON数组及coturn的共享AUTOMATION_TURN_SECRET，执行器为每会话生成限时凭据。媒体和浏览器双方同用该配置；TURN到期后需要手动重新打开媒体。
- 手机实际目标复用资产绑定的组网/改址IPv4和ADB端口。两个网络有相同不可区分的地址时，首版需先修正映射或隔离整套部署；当前控制台不能为某台设备另选Runner，多执行器留到后续。不会尝试另一个地址作为回退。
- ADB密钥卷adb-identity必须保留；换主机或删除密钥可能需要重新配对。数据卷丢失不能视为旧任务从未执行。

## 用户准备流程

1. 在手机手动开启调试并授权。无线调试的配对端口/配对码与连接端口分别填写。
2. 用户点击配对或连接。配对码经stdin传给ADB，不写入journal、URL或进程参数列表；连接成功后读取有限设备属性。
3. 查看画面使用scrcpy。使用脚本或中文控件输入前，用户明确点击“准备脚本控制”，此动作会启动/部署uiautomator2设备辅助组件，并由执行器的受管辅助进程保持连接。inspect操作只读属性，不执行组件安装。执行器重启或手机组件断开后需手动重新准备；存储的历史准备记录不能代表当前可用。
4. 中文输入使用指定或唯一焦点控件的setText，不自动安装/切换输入法、读取剪贴板或解锁密码。特定应用不支持时返回失败，由用户处理。

## 任务与资源规则

- 请求、取消拒收记录和结果写本地SQLite journal；运行参数只保留于内存。重启标记中断或未确认，不自动续跑。
- journal保存稳定executor_id，主服务在派发前绑定此身份；派发、取消、授权及结果回调均核对同一身份。数据卷丢失或更换会得到新身份，旧任务不能在新执行器上被宣称安全取消。每个任务结果包含单调递增revision，防止并发回调乱序覆盖。
- 每台手机仅一个写入控制者，观看可以与脚本并存；关闭画面不取消后台任务。手动控制结束通过stop/关闭会话释放自己的占用。
- 观看会话最长60秒授权，由Rust在对应已打开控制WS内验证后续期。仅存在活动会话时运行到期清理，不做全网查询。
- 每脚本步骤前向主服务确认权限/目标，再运行有限子进程；超时、取消会终止该进程组。已发到手机的动作不能回滚，失败回执不自动重试。
- 直接使用固定版本uiautomator2低层RPC，避免库默认“重新启动服务并重发动作”的包装；这部分版本兼容必须真机验证。
- 取消一个尚未找到的任务也先写持久拒收记录，阻止迟到请求。只有expected_executor_id与已有journal身份一致且此前没有该任务记录时，返回fenced/never_started；普通404和身份变化都不能作为从未执行的证据。已有任务取消后，保留动作outcome_unknown；确认该任务对应的进程与控制占用已结束后，单独返回cleanup_confirmed允许后续新任务。
- 截图只在显式screenshot步骤采集，单张上限8MiB、总额默认100MiB，受内部API与主服务账号权限保护。没有录像、自动失败截图、定时清理或任意shell/Python编辑功能。

## 版本与维护

直接 Python 依赖已固定在 requirements.txt；`aiortc==1.13.0` 搭配具备 Python 3.12 / Linux x86_64 wheel 的 `av==14.2.0`，Docker 构建要求 PyAV 使用预编译包，避免原 `av==14.4.0` 缺 wheel 时在精简镜像上源码编译失败。传递依赖锁与真机 H.264 媒体验证仍待完成。uiautomator2 3.7.1 依据本地已读取仓库 tag。scrcpy-server5.0.1固定并校验摘要；platform-tools固定35.0.2下载URL，独立制品摘要还需在交付验证时固化。

升级前手动停止接纳和完成/取消任务，备份execution.sqlite3及其WAL一致快照、artifacts和ADB密钥。不会自动恢复旧任务。journal或磁盘故障不返回“任务接收成功”。文件容量满时截图步骤明确失败，不掩盖已有任务记录。

截图容量满时可明确删除某个任务的截图：先完成或取消正在运行的任务，再停止执行器，替换下方任务UUID后执行。本命令只删除该任务的截图文件和容量记录，保留任务历史；历史截图链接之后返回不存在。不删journal、取消拒收记录、executor_id或ADB密钥，且锁检测会拒绝与运行中执行器同时操作。

```sh
docker compose stop automation-executor
docker compose run --rm --no-deps automation-executor python -m automation_executor.maintenance --delete-artifacts-for-run 00000000-0000-0000-0000-000000000000
docker compose up -d
```

上述维护命令本轮未执行。进程被强制断电时可能留下尚未登记的截图文件；这些文件也计入截图容量。可在相同停机步骤中把维护参数改为`--delete-unregistered-artifacts`，仅删除未登记且文件名为UUID的截图；不会清理任务或控制占用。升级备份时应同时保存artifacts目录，不直接修改SQLite记录。

待测：首次准备、无线配对端口变化、中文/控件兼容、rotation、1/2/5/10台媒体、TURN、重复命令、取消与回执丢失、进程重启、账号撤销、脚本与人工占用、证据授权及容量。所有性能/可用性数据当前为空。

## 端点变更协议

主服务与执行器需同时升级。`runs/{run}/devices/{device}/withdraw` 只持久撤销该设备，其他设备继续；`targets/{device}/retire` 拒收旧指纹并确认旧准备、会话、控件进程和定向 ADB 连接退出。无全局 `adb disconnect`、无自动续跑或历史输入重放。清理失败、身份变更和未知进程归属均保留待确认，由用户点击重试。

模拟回归：`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v`。测试替换媒体层和设备命令，不证明 Android 或 WebRTC 媒体可用。
