# 镜像构建提速

## 当前状态

2026-10-24：第二阶段优化已随提交 `38fa65f90bd6095e03ac7ee351991a2609d48b26` 推送至 `feat/automation-center`。[自动构建 #23](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165) 已全部成功，amd64 镜像已发布并核实索引摘要，未部署到用户设备。

本轮全流程用时 **13 分 30 秒**，北京时间 15:39:39–15:53:09（UTC 07:39:39–07:53:09）。相对最近旧流程 #22 的 23 分 45 秒减少 **10 分 15 秒，约 43.2%**。这是已有运行的实测对比，不是控制全部变量的基准测试，也不是后续每次构建的固定耗时承诺。

| 运行 | 全流程耗时 | 状态及用途 |
|---|---|---|
| [#20](https://github.com/CiBeDerORG/EasyTier/actions/runs/40033901381) | 37 分 53 秒 | 优化前历史基线 |
| [#21](https://github.com/CiBeDerORG/EasyTier/actions/runs/40069487917) | 28 分 22 秒 | 优化前历史基线 |
| [#22](https://github.com/CiBeDerORG/EasyTier/actions/runs/40073334116) | 23 分 45 秒 | 最近旧流程，三轮 Rust 分别为 6 分 56 秒、7 分 55 秒、6 分 43 秒 |
| [#23](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165) | **13 分 30 秒** | 本轮优化提交，构建、测试、镜像发布全部成功 |

本轮只由既有 push 事件触发 CI；未执行本地测试、构建、类型检查、lint 或验证脚本，未手动触发或重跑工作流。缓存边界与发布回归均在 GitHub Actions 中实际执行，详细证据见[轮次记录](task/test-report.md)。

## 实现范围

- 前端共享库只构建一次，配置生成器复用输入完全一致的 WASM 产物。
- Rust Web 增加镜像内部静态目录模式，页面不再是该模式下的编译输入。
- 原生程序和前端并行构建，按原生输入指纹精确复用四个程序。
- 镜像记录当前组合版本及原生程序实际来源版本。
- 自动构建只选择 amd64，手动入口继续支持选择已部署架构。
- 原生只执行一次 Cargo 发布构建；`easytier-web-embed` 提供目录式网页，`easytier-web` 通过 `exec --no-web` 保留 API-only 默认行为。
- 公开服务回归在独立任务中与原生和前端并行，使用测试专用优化参数；发布必须同时等待镜像烟测与回归测试成功。
- Cargo 工作目录使用兼容依赖前缀恢复、源码指纹精确保存；仅内容一致的跟踪源码恢复缓存时间戳，变化文件由 Cargo 重新编译。
- 选中的符号链接增加、删除或重指向时，整轮跳过时间戳恢复，避免旧目标时间戳掩盖真实依赖变化。
- 完整程序缓存基于实际本地依赖闭包、资源链接目标和构建配方，工作流显示名称不再直接使后端缓存失效。

本轮测试缓存确实未命中，测试程序首次编译用时 11 分 19 秒；20 个发布回归实际执行 0.32 秒且全部通过。成功后已保存测试 Cargo 缓存与 666 项原生输入的时间戳元数据。**下一次热缓存恢复、源码变化后的滚动复用及纯前端变更跳过原生编译的收益，尚未实测。**

| 项目 | 代码状态 | 运行状态 |
|---|---|---|
| 前端共享库、WASM 复用及目录式网页 | 沿用已有实现 | #23 前端任务成功，用时 2 分 24 秒 |
| 单次原生编译与两个 Web 兼容入口 | 已实现 | #23 单次发布构建 9 分 42 秒，原生任务整体 10 分 48 秒；镜像烟测通过 |
| 发布回归独立并行且保留门禁 | 已实现 | #23 20 个测试通过；测试程序冷缓存编译 11 分 19 秒 |
| 完整程序 schema 2 与依赖闭包 | 已实现 | #23 13 个缓存边界测试通过，用时 0.379 秒 |
| Cargo 产物滚动缓存与源时间戳恢复 | 已实现 | #23 首次测试缓存建立并保存成功；未来热缓存收益未实测 |
| 原生产物与镜像来源分别记录 | 已实现 | #23 镜像装配及烟测任务 54 秒，发布成功且已核实镜像索引摘要 |

## 本轮发布结果

- 镜像：`ghcr.io/cibederorg/easytier:feat-automation-center`，架构 `linux/amd64`。
- 索引摘要：`sha256:a9d321843566b9206a5409fddee1d6352eb962ec3b33a171f22685168f66daef`。
- [回归任务日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/job/120816375902)；[回归证据文件](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/artifacts/12552997637)。
- 本次完成镜像发布与验证，未执行设备部署；实机更新后的行为仍需另行验收。

## 入口

- [轮次记录与待测项](task/test-report.md)
- [需求范围](require/requirement.md)
- [构建与缓存设计](plan/plan-01-image-build.md)
- 工作流：.github/workflows/node-attributes-image.yml
- 前端准备：.github/actions/prepare-pnpm/action.yml
- 原生输入及产物记录：.github/scripts/build-native-inputs.py
- 原生构建：.github/scripts/build-native.sh
- 镜像布局：.github/workflows/Dockerfile.node-attributes

## 任务分工

- 根 agent：工作流、独立回归脚本、记录、提交推送与自动构建跟进。
- workflow_source_audit：单次原生编译、API 兼容入口、镜像运行检查。
- ci_cache_reference：原生依赖闭包、缓存指纹、时间戳恢复及 CI 边界用例。
- build_optimization_review：只读交叉审阅工作流、发布门禁与缓存正确性。

