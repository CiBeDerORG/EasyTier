# 构建与缓存设计

## Job 关系

build_web 与 build_native 并行；两者完成后由 build_images 组合镜像，publish 接收本次组合后的镜像。

| 产物 | 输入边界 | 复用方式 |
|---|---|---|
| 配置 WASM | 跟踪的 Rust/proto/资源、Cargo 输入、构建脚本及固定工具版本 | 完整目录精确 key，加逐文件 SHA-256 manifest |
| 原生四程序 | 本地 Cargo 包、非页面资源、协议、锁文件、构建配方、target/allocator/profile 和 runner/toolchain | 完整 bundle 精确 key，加来源 SHA、平台、文件大小和 SHA-256 manifest |
| 页面目录 | 本次前端、共享库和已准备好的 WASM | 每次构建一次共享库，两个消费者分别打包 |
| 最终镜像 | 同次 run 的页面 artifact 和选定架构的原生 artifact | 复制独立目录并记录组合版本 |

第三方 Rust 依赖缓存用于缓存 miss 后的加速，由 Cargo 自行判断哪些 crate 要重编。它与完整可执行文件缓存独立，不用相近 key 的旧可执行文件替代目标产物。

## 来源与失效

- native 与 WASM key 均排除 Git 提交号，避免纯页面提交让后端失效。
- 包括文件路径、内容及构建配置，防止资源、协议或工具版本变化后误用旧产物。
- 生成产物清单前核对原始源码输入快照，拒绝将构建中被修改的输入登记到旧 key。
- 已恢复的 manifest 原样保留原始编译来源。镜像 OCI revision 是当前组合提交，backend revision 是原生实际来源。
- 缓存被淘汰时重新构建；损坏的精确缓存会被拒绝。已有 key 不可覆盖，可通过移除该缓存或提升 schema 恢复后续缓存收益。

## Web 运行模式

- web-static 从 EASYTIER_WEB_DIR 或 --web-dir 指定目录加载页面，编译不读取 frontend/dist。
- 分支镜像目录为 /usr/share/easytier/web，普通 API-only Web 与兼容名称 easytier-web-embed 均保留。
- embed 继续用于单文件嵌入发布；显式指定目录时使用目录且不静默回退。
- 入口 HTML 和配置生成器共用现有服务，保持动态 api_meta.js 与页面入口禁缓存。
