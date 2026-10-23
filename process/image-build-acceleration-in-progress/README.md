# 镜像构建提速

## 当前状态

2026-10-23：首轮自动工作流的 amd64 原生四程序编译与完整产物缓存保存已成功；前端在计算 WASM 输入指纹时失败，未进入页面编译，镜像装配及发布被跳过。已定位为将 easytier/assets 目录符号链接当成普通文件读取，本轮修复链接及其跟踪目标资源的指纹处理，等待修复提交后的自动工作流确认。

修复基于 feat/node-attributes-main 的 c9ff7bb，保留最新的网关组管理还原及中文提交规则。按用户授权修改并 push，由已有 push 事件自动构建当前部署的 amd64 镜像；不执行本地测试、构建、类型检查、lint 或验证脚本，也不手动触发 CI。

## 实现范围

- 前端共享库只构建一次，配置生成器复用输入完全一致的 WASM 产物。
- Rust Web 增加镜像内部静态目录模式，页面不再是该模式下的编译输入。
- 原生程序和前端并行构建，按原生输入指纹精确复用四个程序。
- 镜像记录当前组合版本及原生程序实际来源版本。
- 自动构建只选择 amd64，手动入口继续支持选择已部署架构。

首次运行会建立新缓存；实际耗时及功能结果以后续自动工作流为准。

| 项目 | 代码状态 | 运行状态 |
|---|---|---|
| 前端共享库单次构建 | 已实现 | 被前置 WASM 指纹步骤阻断，待修复后运行 |
| WASM 完整产物与依赖缓存 | 目录链接处理已修复 | 待自动工作流，首次缓存尚未建立 |
| Web 镜像内静态目录 | 已实现 | 原生编译已通过，镜像运行待确认 |
| 原生四程序完整产物缓存 | 已实现 | 3e857980 已编译并保存缓存，后续复用待观察 |
| 前后端并行、单架构镜像装配 | 已实现 | 并行任务已运行，装配因前端失败而跳过 |
| 原生产物与镜像来源分别记录 | 已实现 | 原生 manifest 已生成，镜像标签待发布 |

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
