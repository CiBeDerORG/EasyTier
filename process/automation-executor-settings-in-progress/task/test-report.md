# 执行器设置验证记录

## 第 1 轮 · 2026-10-24

基线：`feat/automation-center@4c816d78`。用户要求直接在自动化中心设置页配置执行器，沿用先完成代码和模拟联调、实机另行验收的安排。

已实现：管理员设置入口、配置读写/检查 API、Token 加密、环境变量兼容、共享配置即时应用、切换保护和旧空闲目标清理。设置页沿用控制台卡片与移动布局，切换页签保留未保存草稿；普通账号不显示入口，接口也拒绝其读写和探测。

模拟验证中发现并处理：

- 测试夹具重复创建同名账号，已改为查找既有账号后再创建。
- HTTP 请求头会裁剪 Token 尾部空格。Rust 请求、Python 认证及结果回调增加可逆 UTF-8 Base64 认证方式，普通 ASCII 凭证继续沿用原 Bearer。带空格、中文、引号等值不被修改或轮换。
- 数据库迁移回退需遵循本仓库保留数据的约定，当前迁移明确拒绝删除已保存凭证，要求恢复升级前备份。
- 本机缺少仓库默认 mold 链接器，KCP bindgen 需要 GCC include 路径；沿用现有记录的 bfd、LIBCLANG_PATH 和 BINDGEN_EXTRA_CLANG_ARGS 适配后完成全部 Rust Web 回归，未修改项目构建配置。

|验证|结果|证据|
|---|---|---|
|`cargo test -p easytier-web`|405 passed、0 failed、2 ignored；包含新增 8 项设置/认证测试和原发现、凭证及资产回归|[Rust 日志](artifacts/rust.log)|
|`pnpm --dir easytier-web/frontend build:ci`|类型检查、Vite 构建通过；已有大包提示仍存在|[前端日志](artifacts/frontend.log)|
|设置与发现浏览器回归|8 passed；保存和读取不探测、检查不重试、凭证保留、权限、草稿、移动宽度、原 ADB 联动|[联合日志](artifacts/browser-and-discovery.log)|
|最终设置页截图回归|4 passed；截图禁用动画后确认成功/失败提示和布局|[最终日志](artifacts/browser-final.log)、[桌面](artifacts/settings-desktop.png)、[手机](artifacts/settings-mobile.png)|
|`python3 -m unittest discover -s tests -v`|11 passed；认证原值往返、旧 Bearer 兼容及原设备退出模拟|[执行器日志](artifacts/executor.log)|

重现 Rust 验证所用环境：

```sh
PATH=/workspace/toolchains/python/cmake/data/bin:/workspace/toolchains/cargo/bin:$PATH \
RUSTUP_HOME=/workspace/toolchains/rustup CARGO_HOME=/workspace/toolchains/cargo \
RUSTFLAGS='-C link-arg=-fuse-ld=bfd' PROTOC=/workspace/toolchains/protoc/bin/protoc \
LIBCLANG_PATH=/workspace/toolchains/python/clang/native \
BINDGEN_EXTRA_CLANG_ARGS='-I/usr/lib/gcc/x86_64-linux-gnu/14/include' \
CARGO_NET_OFFLINE=true cargo test -p easytier-web
```

交付边界：执行器仍需独立部署并填写回调/监听配置；特殊字符 Token 需要主服务与执行器同时更新。本轮新增配置不迁移旧执行器上的截图文件，不改变活动投屏续租行为；媒体依赖的既有安装问题仍待单独修复。

未执行：真实 Android/OpenWrt/fnOS 验收、执行器镜像构建、远端推送、Actions 和生产部署。本轮不会把媒体依赖安装问题写成已修复。

## 第 2 轮 · 2026-10-24 · 推送与镜像发布

用户明确要求「push发布」。本轮将上一轮已验证的代码、测试及交付记录提交到 `feat/automation-center`，通过既有 `EasyTier GHCR Image` push 工作流发布 `ghcr.io/cibederorg/easytier:feat-automation-center`（linux/amd64）。

推送前已同步远端分支；本轮没有新增业务代码，不重复执行上一轮已通过的本地测试。提交、工作流启动、镜像发布和远端摘要已分别核实。

执行器设置提交为 `54b6d1b4`。远端已推进到 `2e01c487`，包含 LuCI 受限账号接入检查修复与镜像构建提速；已合并保留。唯一冲突在 `process/README.md` 的进度表，保留双方条目及远端较新的发布状态，业务代码没有冲突。第 1 轮本地测试对应设置实现与当时基线；合并后的构建与回归以本轮 Actions 结果为准。

发布结果：

|核验项|结果|证据|
|---|---|---|
|代码推送|`feat/automation-center` 已推进至 `72119aeec3b566e9761b32d5e57f553c8218562a`，发布完成后再次 fetch 确认远端一致|[提交](https://github.com/CiBeDerORG/EasyTier/commit/72119aeec3b566e9761b32d5e57f553c8218562a)|
|push 自动构建|#26 成功，13 分 45 秒；前端、原生程序、原生发布/LuCI 回归、镜像装配与启动检查、发布任务均成功|[Actions](https://github.com/CiBeDerORG/EasyTier/actions/runs/40084382069)、[状态回执](artifacts/release-ci.json)|
|镜像发布|分支标签 `ghcr.io/cibederorg/easytier:feat-automation-center` 已更新，平台 `linux/amd64`|[镜像回执](artifacts/publication.json)|
|远端内容核验|匿名读取 GHCR manifest、子 manifest 和配置；逐项核对响应字节摘要、平台，以及镜像/前端/原生程序提交号；分支标签和固定提交标签摘要一致|[镜像回执](artifacts/publication.json)|

固定提交标签：`ghcr.io/cibederorg/easytier:sha-72119aeec3b566e9761b32d5e57f553c8218562a`。

已核实摘要：`sha256:067e52cb982acfeb05bc4508b73b9665d3a055588fcc6229eadecdc6c6f77d5d`。镜像、前端和原生程序提交号均为 `72119aeec3b566e9761b32d5e57f553c8218562a`；前端资源使用目录模式。

本机未执行 Docker 拉取/运行；上述匿名远端核验范围是 manifest 与镜像配置。容器启动和镜像拉取检查由成功的发布工作流执行。未读取需要 GitHub 登录的完整任务日志，因此不将历史测试条数写成本轮 CI 条数。

本轮未执行：重复本地测试、类型检查或构建，真实 Android/OpenWrt/fnOS 验收、独立执行器镜像构建和实机部署。主镜像不包含独立 Python 执行器；特殊字符 Token 的双端更新要求及媒体依赖待处理边界仍适用。发布后的补记仅包含文档与公开状态/摘要证据，沿用 `process/**` 忽略规则，不重复触发镜像构建。

## 2026-10-25 执行器容量迁入 EasyTier 设置

- 需求：管理员在「自动化中心 → 设置 → 执行器容量」配置运行并发、待处理任务、投屏会话、投屏启动并发；只提供建议值，不使用此前 5/30/10/2 等任意固定上限。
- 设计：新增独立的管理员专用 GET/PUT `/api/v1/automation/settings/capacity`，Rust 转发到已配置且认证的执行器 `/internal/v1/settings/capacity`，核验 executor_id，保存时执行 revision CAS。
- 执行器：容量保存在已有 `execution.sqlite3` 的 metadata；变更 `FairDeviceSlots` 可直接增加/减少运行槽，媒体启动槽也改为动态公平槽，媒体会话数和 pending limit 读取运行时配置。收缩容量时不终止正在运行的设备动作。容器重启后从 SQLite 恢复。
- 前端：新增 `ExecutorCapacitySettings.vue`，展示推荐值 5/30/10/2，允许输入更大正整数并在超过推荐时提示资源风险；脚本运行/重跑取消写死的五路并发 cap。
- 部署：`CiBeDerORG/deploy` main 同步移除部署脚本四个资源参数硬范围，保留首次安装 env 默认值；页面保存后以执行器数据库为准。
- 验证：新执行器配置热更新、重启持久化、降低并发不打断已执行作业等自动化单元测试已加入。首次执行器 CI #40185082950 中新测试通过，旧 PhantomRegistrationTests 的 SimpleNamespace 依赖未同步导致 3 项失败；已修复后重新启动 #40185248849，需以后续真实结果为准。
- 风险/待验收：主服务 Rust/Vue CI 结果及 GHCR 完整发布待确认；FnOS 实际数据卷持久化、管理员鉴权、最大资源压力仍待真机验证。不得将用户可填较大的数解释为无限 CPU/内存。
