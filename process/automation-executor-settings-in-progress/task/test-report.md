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

## 第 2 轮 · 推送与镜像发布

用户明确要求「push发布」。本轮将上一轮已验证的代码、测试及交付记录提交到 `feat/automation-center`，通过既有 `EasyTier GHCR Image` push 工作流发布 `ghcr.io/cibederorg/easytier:feat-automation-center`（linux/amd64）。

推送前已同步远端分支；本轮没有新增业务代码，不重复执行上一轮已通过的本地测试。提交、工作流启动、镜像发布和远端摘要分别核实后补充回执。

执行器设置提交为 `54b6d1b4`。远端已推进到 `2e01c487`，包含 LuCI 受限账号接入检查修复与镜像构建提速；已合并保留。唯一冲突在 `process/README.md` 的进度表，保留双方条目及远端较新的发布状态，业务代码没有冲突。第 1 轮本地测试对应设置实现与当时基线；合并后的构建与回归以本轮 Actions 结果为准。
