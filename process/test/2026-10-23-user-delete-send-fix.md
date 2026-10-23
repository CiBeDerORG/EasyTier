# 用户删除接口编译失败修复与复测

## 范围

- 日期：2026-10-23（Asia/Shanghai）。
- 仓库：`CiBeDerORG/EasyTier`。
- 分支：`feat/node-attributes-main`。
- 修复前分支提交：`7a3d09ea0f3e71f3591d6cdf12bb2d725c856c06`。
- 用户指令：在上一轮 GitHub Actions 报错基础上“看下哪里问题，改一下”。本轮继续修复该阻塞并验证修复结果。
- 原始失败运行：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496>，测试提交 `9dc37dd0cebc32696c41cddc0b4fd75f3a435db3`。

## 问题与修复

原运行在 `easytier-web/src/restful/user_admin.rs:145` 注册删除用户路由时出现 Rust `E0277`：`delete_user` 不满足 Axum `Handler<_, _>` 约束。后端未能编译，测试用例没有开始执行。

调用链为 `delete_user` → `CentralNetworkService::disable_deleted_user_runtime` → `ClientManager::stop_networks_for_deleted_user`。最后一个方法在 `timeout(..., async { ... })` 中跨 `.await` 借用了局部 RPC client。

`SessionRpcClient` 类型仅保留 `Send`，没有 `Sync`，生成的 service trait 也没有 `Sync` supertrait。异步块保存的 `&SessionRpcClient` 不能满足 `Send`，因而使上层 handler future 无法满足 Axum 的要求。

修复文件：`easytier-web/src/client_manager/mod.rs`，将该局部异步块改为 `async move`：

```diff
- let stopped = tokio::time::timeout(Duration::from_secs(8), async {
+ let stopped = tokio::time::timeout(Duration::from_secs(8), async move {
```

RPC client 所有权进入异步块，而非跨 await 借用。该变量在 timeout 后没有继续使用；8 秒 RPC 超时、查询/删除/再次查询顺序、会话身份校验及停网确认条件保持原有行为。

同时给专用工作流增加包含分支与提交号的 `run-name`，便于区分本轮实测版本，并由本次工作流文件变更触发修复后的 CI。自动触发范围仍限定当前分支及该工作流文件，普通源码与报告提交不自动触发。

## 复测结果

修复已准备，等待提交并触发 GitHub Actions。实际提交、运行链接和最终结果将在运行完成后补充；当前不能宣称编译或测试通过。

计划沿用既有 Web 回归工作流：

| 检查 | 命令 | 本轮结果 |
| --- | --- | --- |
| Web 后端测试 | `cargo test --locked -p easytier-web --no-fail-fast` | 待执行 |
| 前端单元测试 | `pnpm --dir easytier-web/frontend-lib test:config-ui` | 待执行 |
| 前端类型检查及构建 | `pnpm --dir easytier-web/frontend build` | 待执行 |
| 网络配置映射 | `node easytier-web/frontend-lib/scripts/test-network-config.mjs` | 待执行 |
| 节点属性浏览器回归 | 在 frontend 目录执行 `node --test tests/node-attributes.test.mjs` | 待执行 |

## 验证边界

本轮针对删除用户调用链的编译阻塞。现有回归不替代新增用户管理全部业务专项验收，真实设备停网、离线/超时反馈、OIDC/旧会话拒绝及删除与并发写入等场景仍按原需求记录管理。

原始失败证据继续保留在 `process/test/2026-10-23-node-attributes-main-github-actions.md`。本轮没有部署服务或操作实际用户数据。
