# 镜像构建提速轮次记录

## 2026-10-23 实施轮次

### 授权与范围

- 用户在构建耗时分析后要求“开始处理”，本轮实现重复构建消除、WASM 缓存、原生程序完整产物复用及镜像内独立页面资源。
- 当前分支：feat/node-attributes-main；修改基线：1576c8b8aad8694f663807894ad5be8028584507。
- 按改 BUG 模式执行：修改后 push，由已有 push 触发器自动构建当前部署的 amd64；不手动触发构建，不运行本地测试、构建、类型检查、lint 或验证脚本。
- 四个 agent 按前端、Rust Web、原生流水线和只读审阅分工，互不覆盖写入。

### 代码改动

1. Web 专用 CI 先构建一次 frontend-lib，再使用两个页面的 build:ci；保留单独开发构建的前置步骤。
2. WASM 使用独立 Rust 依赖缓存及完整产物缓存，精确固定 Rust 1.95.0、wasm-pack 0.15.0 和 protoc 35.1。输入指纹与产物 manifest 校验通过后，跳过 Rust/WASM 工具安装、编译和 wasm-opt。
3. 新增 web-static feature、--web-dir / EASYTIER_WEB_DIR，由原 Web HTTP 服务读取镜像内目录。动态 api_meta.js、同源 API、HTML 禁缓存、config-generator、原端口模式继续由同一服务处理。
4. 无目录或必要页面资源缺失时在启动监听器前报错；目录模式不回退旧的嵌入资源。原 embed feature 继续支持独立内嵌发布。
5. 原生 build_native 与 build_web 并行；输入完全一致时复用完整的四程序产物，并跳过原生编译环境准备。缓存不使用近似 key，缺损或校验不符时重新编译。
6. 原生输入覆盖 Cargo 包全部跟踪源码/资源、协议、迁移、翻译、锁文件、Cargo 配置、构建配方、target/allocator、固定 Rust 版本及 runner 镜像版本，排除三个页面目录。
7. 两轮原生构建对齐 easytier allocator features；Core/CLI/普通 Web 与目录版 Web 分别保存为四个现有程序入口。
8. 分支镜像使用独立 Dockerfile，分别复制程序、页面和原生 manifest。镜像 revision 标识本次组合提交，另记录原生实际源码 SHA 和输入指纹，缓存命中时不改写旧程序的版本来源。
9. 自动 push 仍只出 amd64；手动入口可选部署架构。仅文档变更跳过镜像工作流。
10. Cargo.lock 按 tower-http 0.7.1 的 fs feature 及官方 crates.io 索引补充 http-range-header 0.4.2 和相应依赖引用，没有执行 Cargo 解析或更新命令。

### 实际执行结果

- 已进行源码和 diff 的人工阅读及 agent 交叉审阅。
- 审阅后补充：生成 WASM / native manifest 前重新比较跟踪源码快照，防止准备或编译步骤改写 Cargo.lock 后错用旧 key；移除在 shared-key 模式下不会生效的 Rust dependency cache key 参数。
- 测试：未执行（改 BUG 模式）。
- 本地构建、类型检查、lint、YAML/脚本验证、缓存脚本运行：未执行（改 BUG 模式）。
- 手动 Actions / CI：未触发。
- 本轮性能收益尚未实测；历史 16 分 32 秒原生构建、约 2 分 23 秒 WASM 和 37 秒重复库构建仅用于定位，不能作为本轮已达到的结果。
- 提交后自动构建的结果以后续 Actions 页面为准；首次新输入需要建立缓存。

### 待观察范围

- 首次 amd64 自动构建：新静态目录 feature、锁文件和原生编译是否完成，镜像内四程序是否通过已有工作流检查。
- 下一次后端输入不变的页面修改：原生产物是否显示精确复用并跳过 native prepare/build，WASM 是否复用。
- Rust、协议、资源、锁文件或构建配方变化：相关缓存是否自动失效。
- 页面首页、动态 api_meta.js、配置生成器及其 JS/WASM 资源；现有同端口、拆分端口、--no-web 和 embed 模式。
- 飞牛现有 deploy 启动命令是否直接适配新版镜像。
- 镜像 SHA 与原生版本不同的情况：镜像标签及 build-manifest.json 是否清楚记录各自来源。

### 已知边界

- 缓存失效或被清理时自动重建；首次构建不能反映纯页面修改的热缓存耗时。
- 输入范围保守，部分不影响当前 target 的 Rust 包修改也会导致重建。
- GitHub 精确 key 的缓存不可覆盖；异常缓存会继续回退重建，避免使用损坏产物，可通过提升缓存 schema 换新 key。
- 从镜像取出的目录版 easytier-web-embed 需要同时携带静态页面目录；独立单文件发布继续使用 embed feature。
- 没有调整 LTO、优化级别或 Cargo 并发，也没有新增构建服务器。

## 2026-10-23 第二轮：修复 WASM 输入指纹中的目录符号链接

### 失败证据与原因

- 用户报告自动构建失败，本轮只读取已有 Actions 状态和日志，没有手动触发或重跑任务。
- [首轮构建 39951785314](https://github.com/CiBeDerORG/EasyTier/actions/runs/39951785314)，提交 3e857980：`Build Web and config generator` 在北京时间 20:14 的 `Compute config-generator WASM inputs` 步骤失败。错误为 `IsADirectoryError: [Errno 21] Is a directory: .../easytier/assets`。
- [后续构建 39952930853](https://github.com/CiBeDerORG/EasyTier/actions/runs/39952930853)，提交 fd42e60：已读取的前端任务日志在北京时间 20:37 出现相同错误。
- `easytier/assets` 是 Git mode 120000 的符号链接，指向仓库根目录 `assets`。新增的 WASM 指纹脚本无条件使用 `Path.read_bytes()`，跟随链接后尝试把目录当文件读取，导致前端准备阶段退出；该错误由上一轮加速改动引入。
- 首轮 amd64 原生四程序编译、原生 manifest 生成和完整产物缓存保存均已成功。该结果仅证明 3e857980 的原生构建及缓存保存成功；前端未编译，镜像装配、冒烟步骤和发布均被跳过。

### 修复范围

- 修复基于远端最新 c9ff7bbc04ae05cfce029629e7e30990d95dedfc，保留用户后续的网关组管理还原及中文提交规则。
- `web-build-inputs.py` 按 Git 文件类型分别处理：普通文件读取内容，符号链接读取链接目标文字，拒绝不支持的输入类型。
- 同时将链接指向的仓库内跟踪文件或目录下跟踪资源补入输入列表，包含根 `assets` 图片以及 README/LICENSE 文件链接目标；只使用 Git 跟踪清单，避免临时文件进入缓存 key。
- 目标中的符号链接继续展开并去重；链接目标超出仓库或没有可跟踪输入时明确失败，避免产生不完整的缓存指纹。
- 构建前和缓存保存前仍共用同一套输入快照逻辑。脚本自身包含在指纹输入中，修复会生成新的 WASM key。
- 本轮不修改两个 Web 程序的打包方式，管理网页及其资源保持原有构建方案。

### 执行结果与待观察项

- 已读取 Git 跟踪类型、链接目标、源码及已有 Actions 日志，并进行源码 diff 人工交叉审阅。
- 本地测试、构建、类型检查、lint、语法检查、指纹脚本运行：未执行（改 BUG 模式）。
- 手动 Actions / CI：未触发；修复 push 后按已有规则自动构建 amd64。
- 修复后自动构建结果：待确认；不得将历史原生构建成功视为本次完整镜像成功。
- 待观察：WASM 输入计算通过；首次 WASM 编译与产物缓存保存完成；前端页面打包完成；镜像装配、已有自动冒烟步骤及发布完成。
