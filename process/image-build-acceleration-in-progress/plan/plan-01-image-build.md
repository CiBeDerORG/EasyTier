# 构建与缓存设计

## Job 关系

build_web、build_native 与 test_native 并行；前两者完成后由 build_images 组合并烟测镜像。publish 必须同时等待 build_images 和 test_native 成功，任何一个失败或被跳过都不会发布。

| 产物 | 输入边界 | 复用方式 |
|---|---|---|
| 配置 WASM | 跟踪的 Rust/proto/资源、Cargo 输入、构建脚本及固定工具版本 | 完整目录精确 key，加逐文件 SHA-256 manifest |
| 原生四程序 | 本地 Cargo 包、非页面资源、协议、锁文件、构建配方、target/allocator/profile 和 runner/toolchain | 完整 bundle 精确 key，加来源 SHA、平台、文件大小和 SHA-256 manifest |
| 页面目录 | 本次前端、共享库和已准备好的 WASM | 每次构建一次共享库，两个消费者分别打包 |
| 最终镜像 | 同次 run 的页面 artifact 和选定架构的原生 artifact | 复制独立目录并记录组合版本 |

完整原生程序包使用 schema 2 的精确输入缓存。Cargo 工作目录另外使用 `actions/cache/restore` 和 `actions/cache/save`：依赖配方形成稳定恢复前缀，源码指纹形成保存键；每个新源码版本编译成功后保存新快照。首次可以从旧 release 依赖缓存迁移，但旧 rust-cache 动作禁止 post 清理和保存，避免覆盖新工作目录。

Cargo 缓存中的 source-mtimes 元数据只用于恢复路径、Git 模式和内容哈希完全一致的当前跟踪文件时间戳。代码变化、缺少记录或无效记录不会恢复旧时间戳。该操作不修改源码字节。依赖缓存仍由 Cargo 检查有效性，近似恢复的 target 不能直接当作完整程序包发布。

test_native 使用独立缓存前缀，键额外包含测试脚本。测试命令单独覆盖 release 的 LTO、codegen units 和优化级别；不会改变生产构建的 Cargo 配置。即使原生程序包精确命中，当前源码的回归任务仍执行并约束发布。

## 来源与失效

- native 与 WASM key 均排除 Git 提交号，避免纯页面提交让后端失效。
- 包括文件路径、内容及构建配置，防止资源、协议或工具版本变化后误用旧产物。
- 从 easytier 和 easytier-web 递归解析本地 path/workspace 依赖，覆盖完整包资源及仓库内符号链接目标；不再纳入整个工作流文件或无关平台包源码。
- 生成产物清单前核对原始源码输入快照，拒绝将构建中被修改的输入登记到旧 key。
- 已恢复的 manifest 原样保留原始编译来源。镜像 OCI revision 是当前组合提交，backend revision 是原生实际来源。
- 缓存被淘汰时重新构建；损坏的精确缓存会被拒绝。已有 key 不可覆盖，可通过移除该缓存或提升 schema 恢复后续缓存收益。

## Web 运行模式

- web-static 从 EASYTIER_WEB_DIR 或 --web-dir 指定目录加载页面，编译不读取 frontend/dist。
- 分支镜像目录为 /usr/share/easytier/web，普通 API-only Web 与兼容名称 easytier-web-embed 均保留；只编译一次 web-static，普通入口用轻量 shell exec 添加 --no-web，参数边界、退出码和信号传递在镜像烟测验证。
- embed 继续用于单文件嵌入发布；显式指定目录时使用目录且不静默回退。
- 入口 HTML 和配置生成器共用现有服务，保持动态 api_meta.js 与页面入口禁缓存。

