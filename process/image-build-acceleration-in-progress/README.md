# 镜像构建提速

## 当前状态

2026-10-24：第二阶段优化及后续回归脚本修正已完成。镜像对应提交 `f37049eccfcde64df0abd59cd86f2cc365ab4b74` 的 [自动构建 #25](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133) **全部成功并发布 amd64 镜像**，已匿名拉取核实索引、架构和提交标签，未部署到用户设备。此前 [#23](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165)、[#24](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493) 也均成功。

最新 #25 全流程为 **10 分 56 秒**，北京时间 **16:15:20–16:26:16**（UTC **08:15:20–08:26:16**）。相对最近旧流程 #22 的 23 分 45 秒减少 **12 分 49 秒，约 54.0%**。首次优化运行 #23 为 13 分 30 秒，包含后端 LuCI 改动的 #24 为 12 分 12 秒。这些运行的源码与缓存状态不同，只作实际运行对照，不是控制全部变量的基准测试或每次固定耗时承诺。

| 运行 | 全流程耗时 | 状态及用途 |
|---|---|---|
| [#20](https://github.com/CiBeDerORG/EasyTier/actions/runs/40033901381) | 37 分 53 秒 | 优化前历史基线 |
| [#21](https://github.com/CiBeDerORG/EasyTier/actions/runs/40069487917) | 28 分 22 秒 | 优化前历史基线 |
| [#22](https://github.com/CiBeDerORG/EasyTier/actions/runs/40073334116) | 23 分 45 秒 | 最近旧流程，三轮 Rust 分别为 6 分 56 秒、7 分 55 秒、6 分 43 秒 |
| [#23](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165) | **13 分 30 秒** | 本轮优化提交，构建、测试、镜像发布全部成功 |
| [#24](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493) | **12 分 12 秒** | 后端 LuCI 修复；原生、测试均恢复滚动缓存，构建和发布全部成功 |
| [#25](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133) | **10 分 56 秒** | 全部成功并发布；原生精确整包复用仅 13 秒、无 Cargo 重编；新分区的冷测试编译 9 分 14 秒 |

本轮只由既有 push 事件触发 CI；未执行本地测试、构建、类型检查、lint 或验证脚本，未手动触发或重跑工作流。缓存边界与发布回归均在 GitHub Actions 中实际执行，详细证据见[轮次记录](task/test-report.md)。

## 实现范围

- 前端共享库只构建一次，配置生成器复用输入完全一致的 WASM 产物。
- Rust Web 增加镜像内部静态目录模式，页面不再是该模式下的编译输入。
- 原生程序和前端并行构建，按原生输入指纹精确复用四个程序。
- 镜像记录当前组合版本及原生程序实际来源版本。
- 自动构建只选择 amd64，手动入口继续支持选择已部署架构。
- 原生只执行一次 Cargo 发布构建；`easytier-web-embed` 提供目录式网页，`easytier-web` 通过 `exec --no-web` 保留 API-only 默认行为。
- 公开服务回归在独立任务中与原生和前端并行，使用测试专用优化参数；发布必须同时等待镜像烟测与回归测试成功。
- publication 与 LuCI 回归合并为一次 Cargo/libtest 调用，按两个筛选的并集执行；#25 已验证一次编译、一次测试进程，31 个选中测试全部通过。
- Cargo 工作目录使用兼容依赖前缀恢复、源码指纹精确保存；仅内容一致的跟踪源码恢复缓存时间戳，变化文件由 Cargo 重新编译。
- 选中的符号链接增加、删除或重指向时，整轮跳过时间戳恢复，避免旧目标时间戳掩盖真实依赖变化。
- 完整程序缓存基于实际本地依赖闭包、资源链接目标和构建配方，工作流显示名称不再直接使后端缓存失效。

#23 是测试冷缓存运行，首次测试编译 11 分 19 秒，20 个 publication 回归在 0.32 秒内全部通过。#24 已实测恢复 #23 的原生滚动缓存 1,346,584,466 字节和测试缓存 738,927,398 字节，两者均恢复 664 项未改变源码输入时间戳，并成功保存新缓存。**缓存恢复已验证，但不等于项目代码或测试程序完全免编译**：#24 的两次 Cargo 测试调用仍分别编译 3 分 13 秒和 3 分 07 秒，因此又实施了单次调用修正。

#25 已消除第二次 Cargo 调用，但本轮测试使用的新兼容分区**确实未命中缓存**，没有可恢复的时间戳元数据，测试程序仍冷编译 9 分 14 秒。这是本轮全流程的主要等待；31 个 Rust 测试实际执行仅 0.69 秒。成功后已保存新分区缓存与 666 项时间戳，不能把本轮说成“热缓存测试只需三分钟”或承诺后续每轮固定 10 分 56 秒。

环境变量 `ImageVersion` 的记录值也参与缓存指纹；#24 原生和测试报告记录的该值不同，已确认这直接导致两个兼容分区不同。不过，两任务的启动摘要显示相同版本，现有证据不能解释上游记录差异，不能据此认定实际运行镜像不同。后续运行不能只看“恢复缓存”步骤成功就认定命中，应以真实 key、恢复字节数和日志为准。

| 项目 | 代码状态 | 运行状态 |
|---|---|---|
| 前端共享库、WASM 复用及目录式网页 | 沿用已有实现 | #25 前端任务成功，用时 1 分钟 |
| 单次原生编译与两个 Web 兼容入口 | 已实现 | #24 编译步骤 9 分 16 秒，原生任务整体 10 分 30 秒；构建、镜像烟测与发布通过 |
| 发布回归独立并行且保留门禁 | 已实现并通过单次调用验证 | #25 一次 Cargo 执行 20 项 publication 和 11 项 LuCI，31 项全部通过；测试任务整体 9 分 59 秒 |
| 完整程序 schema 2 与依赖闭包 | 已实现 | #25 13 个缓存边界测试通过，用时 0.474 秒 |
| 后端输入不变时的完整程序包复用 | 已实现并获实测 | #25 原生任务 13 秒，27,244,395 字节完整包恢复 2 秒；准备、Cargo 与原生保存步骤全部跳过 |
| Cargo 产物滚动缓存与源时间戳恢复 | 已实现并获实测 | #24 恢复 664 项时间戳并保存新版本；#25 测试新分区未命中，已建立新缓存 |
| 原生产物与镜像来源分别记录 | 已实现并验证 | #25 镜像装配及烟测 50 秒；发布后已核实镜像提交与实际后端来源分别记录 |

## 发布记录

- 镜像：`ghcr.io/cibederorg/easytier:feat-automation-center`，架构 `linux/amd64`。
- #25 当前已发布并匿名拉取核实的索引摘要：`sha256:36c2fca0d951ea6c710d5322ff9426ade9b1306bb4765582db8242119e57e535`。
- `linux/amd64` 子摘要：`sha256:67b25dd6900631b737f2b3bb2b15cff6a4f16b266eb92d4a4859628ebd644c9d`；镜像 revision 为 `f37049eccfcde64df0abd59cd86f2cc365ab4b74`。
- [#23 回归任务日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/job/120816375902)；[#23 回归证据文件](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/artifacts/12552997637)；[#24 回归任务日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493/job/120820881668)。
- [#25 回归任务](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823368047)、[装配与烟测任务](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823584855)、[发布任务](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120825356374) 均已成功。
- #25 复用包的真实后端来源仍为 `724d09bc113fad29d35fb5dfc3dd650ff76e75c4`，保留 #24 原生产物来源，没有将其改写为本轮测试脚本提交。
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

