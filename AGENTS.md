# EasyTier：AI 协作与架构规则

本文件是仓库内 AI 编码助手的入口，适用于整个仓库。具体任务的明确要求优先；若子目录存在 `AGENTS.md`，同时读取并遵循其适用规则。规范使用中文，代码标识符沿用项目的英文术语。

## 开始任务前必须读取

1. [架构索引与变更流程](docs/architecture/README.md)。
2. [统一命名规范](docs/architecture/naming.md)。
3. [需求、计划与任务协作规则](process/README.md)。涉及已有需求时，继续读取对应需求、计划、任务明细和[任务看板](process/task/README.md)。
4. 根据实际改动路径加载下表中的规范；跨边界改动必须读取所有相关规范。

| 改动范围 | 必读规范 |
| --- | --- |
| `easytier-web/frontend/`、`easytier-web/frontend-lib/`、`easytier-web/config-generator/`、`easytier-gui/` 中的 Vue/TS/CSS、`easytier-contrib/easytier-uptime/frontend/` | [前端规范](docs/architecture/frontend.md) |
| 所有 Rust、`easytier-proto/proto/`、FFI/JNI、`easytier-go/`、`easytier-js/` 运行时与平台适配、`tauri-plugin-vpnservice/` 原生与桥接代码 | [后端规范](docs/architecture/backend.md) |
| 用户可见的页面、CLI 交互、配置项、错误提示、权限、默认行为、连接状态、导入导出 | [产品设计思维与规范](docs/architecture/product-design.md)，并读取实现所涉及的前端或后端规范 |
| 构建脚本、CI、Cargo feature、跨平台支持 | [后端规范](docs/architecture/backend.md)及相关包清单、现有 CI；影响前端构建时追加前端规范 |

## 不得破坏的边界

- **MUST** 遵循 [Core 架构](docs/core-architecture.md) 与 [领域上下文](CONTEXT.md) 中的现有所有权和兼容性要求；新规范是补充，不能据此另建一套核心架构。
- **MUST** 保持主要依赖方向 `easytier → easytier-core → easytier-proto`。Core 通过 Host 能力请求平台操作，不能直接创建原生 socket、TUN、路由或系统服务。
- **MUST** 在 Web、GUI、配置生成器等已有共享 UI 消费方复用 `frontend-lib` 的组件与配置模型；共享组件需要宿主后端能力时使用 `RemoteClient`。配置生成器的配置 WASM 保持直接适配路径；独立 uptime 前端、Go/JS Host 不要求引入该库。
- **MUST** 保护线协议、TOML、管理 API、CLI 和公开 SDK 的兼容性；已有 `snake_case` 契约字段不得为了前端风格改成 `camelCase`。
- **MUST** 修改生成代码的源 schema 或生成器，使用现有生成步骤更新产物；禁止手改生成文件、关闭检查或修改测试断言来掩盖错误。
- **MUST** 保留用户已有改动，控制任务范围；风格规范用于新增和实际修改的代码，不授权全库重命名、替换技术栈或重构。
- **MUST** 不在日志、截图、测试产物、文档示例或提交中暴露真实网络密钥、凭据私钥、会话令牌；示例使用明确的虚构值。
- **SHOULD** 优先复用有明确所有权的现有模块。新增抽象必须承担独立策略、状态或生命周期，不能只转发另一个方法。

## 执行与交付

### 全局文档与多 Agent 规则

- **MUST** 每次需求分析形成或更新 `docs/` 下的文档，默认使用 `docs/requirements/<feature>.md`；实施计划放在 `process/plan/`，任务拆解、看板和任务明细放在 `process/task/`，不得只留在对话中。
- **MUST** 需求、计划、任务使用稳定标识和相对链接关联；遵守 [process/README.md](process/README.md) 的模板、状态和交付规则。需求描述目标与验收，计划描述实施顺序，任务明细描述可执行工作包。
- **MUST** 优先拆分低耦合、写入范围互不重叠、可独立验证的任务，以便多个 Agent 同时工作；明确依赖、输入/输出契约、唯一负责人和共享文件写入者。存在真实依赖时按顺序执行，不为了并行强行拆散同一事务或状态所有权。
- **MUST** 依赖各模块的联调与整体验收单独建任务，安排在其前置工作交付之后；各实现任务先完成自身编译与针对性验证，不能将所有验证留到联调。
- **MUST** 任务明细是执行状态的事实源；协调者维护看板并同步任务状态。认领、阻塞、交付和完成时更新记录；写完计划不代表实现已经完成。详细操作见 [协作规则](process/README.md)。

### 交付检查

1. 先检查 `git status --short`、相关源码、清单、测试及 CI，说明本次修改的边界和兼容性影响。
2. 按改动范围选择检查；使用各子项目已有脚本和锁文件，不假设仓库根目录存在通用 `pnpm test`。
3. 涉及公开协议、持久化语义或模块所有权变化时，在改动中记录决策、迁移/回退方式，并更新受影响文档。
4. 区分通过、失败、跳过和未执行。受权限或工具链限制的检查必须注明原因；不能用 mock 或零测试运行宣称端到端验证成功。
5. 交付说明应包含：改了什么、为何属于该层、验证证据、兼容性和剩余限制。纯文档变更检查链接、路径和命令引用即可，无需全量构建。

`MUST` 表示必须，`SHOULD` 表示应遵循且偏离时解释原因，`MAY` 表示可选。若规范、现有实现和任务要求冲突，先核实来源并说明具体差异，不自行静默重写契约。这些文档是 AI 与人工评审规则，不代表已经配置了自动 lint 或 CI 门禁。
