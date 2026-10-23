# 镜像构建提速

## 当前状态

2026-10-23：代码实现与源码交叉审阅完成，性能及运行结果待后续工作流确认。基于 feat/node-attributes-main 的 1576c8b，按用户授权修改并 push，由已有 push 事件自动构建当前部署的 amd64 镜像。本轮不执行本地测试、构建、类型检查、lint，也不手动触发 CI。

## 实现范围

- 前端共享库只构建一次，配置生成器复用输入完全一致的 WASM 产物。
- Rust Web 增加镜像内部静态目录模式，页面不再是该模式下的编译输入。
- 原生程序和前端并行构建，按原生输入指纹精确复用四个程序。
- 镜像记录当前组合版本及原生程序实际来源版本。
- 自动构建只选择 amd64，手动入口继续支持选择已部署架构。

首次运行会建立新缓存；实际耗时及功能结果以后续自动工作流为准。

| 项目 | 代码状态 | 运行状态 |
|---|---|---|
| 前端共享库单次构建 | 已实现 | 未验证 |
| WASM 完整产物与依赖缓存 | 已实现 | 首次缓存待建立 |
| Web 镜像内静态目录 | 已实现 | 未验证 |
| 原生四程序完整产物缓存 | 已实现 | 首次缓存待建立 |
| 前后端并行、单架构镜像装配 | 已实现 | 待自动工作流 |
| 原生产物与镜像来源分别记录 | 已实现 | 待自动工作流 |

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

- 根 agent：原生缓存、流水线、镜像、版本记录及交付文档。
- polling_control：Rust 静态目录模式。
- audit_polling_data：前端单次构建和 WASM 缓存。
- attributes_fix：原生输入边界和最终 diff 的只读复审。
