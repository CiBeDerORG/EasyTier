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

## 第一次修复验证：编译通过，发现旧测试数据遗漏

修复提交：`0ab3ed5c78c8194207695e07015b4b28a4cfa57e`，`fix(web): move deleted-user RPC client into Send future`。

运行：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39899719202>，2026-10-23 12:19:36（Asia/Shanghai）触发。本轮最终失败，但原来的 Rust E0277 编译错误已消除，后端实际执行了 341 个测试。

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| Web 后端测试 | `cargo test --locked -p easytier-web --no-fail-fast` | 编译通过；338 通过、1 失败、2 ignored |
| 前端单元测试 | `pnpm --dir easytier-web/frontend-lib test:config-ui` | 通过：10 个文件、57 个测试 |
| 前端类型检查及构建 | `pnpm --dir easytier-web/frontend build` | 通过 |
| 网络配置映射 | `node easytier-web/frontend-lib/scripts/test-network-config.mjs` | 通过：10 组断言 |
| 节点属性浏览器回归 | 在 frontend 目录执行 `node --test tests/node-attributes.test.mjs` | 通过：13/13，0 失败、0 跳过、0 取消，约 12.92 秒 |

后端 job：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39899719202/job/120238251075>；日志 artifact：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39899719202/artifacts/12478427617>。

前端 job：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39899719202/job/120238250786>；日志 artifact：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39899719202/artifacts/12477939478>。

### 唯一失败用例与第二处修复

失败用例为 `client_manager::session::runtime_revision::tests::managed_runtime_mutation_preserves_base_only_for_patch_scope`。在本次测试提交的 `easytier-web/src/client_manager/session/runtime_revision.rs:1951`，它期待 `begin_managed_runtime_mutation(...).await` 返回 true。

该用例建立空的内存数据库，然后在 `StorageToken` 和 `ReconcileRound` 中硬编码 `user_id: 7`，没有创建用户记录。新增的 `runtime_config_owner_is_active` 会读取数据库，正确拒绝不存在的用户；仅设置会话的 `Authorized` 状态不足以满足这一前提。

修复采用同文件其他正常用例已有的模式：在建立 storage 后调用 `storage.db().auto_create_user(&req.user_token).await.unwrap().id` 创建真实的测试账号，并让 token 和 round 均使用返回的用户 ID。原来的 Patch/Full 成功断言、base revision、epoch 和 mutation fence 检查全部保留；生产代码的用户有效性保护保持原样。

专用工作流同步补充本轮验证说明，用该工作流文件变更触发包含两处修复的新提交；触发条件与测试命令保持原样。

## 第二次修复验证

测试数据修复准备提交。本轮新运行尚未开始，不能宣称修复后全部通过。将对包含两处修复的提交重新执行上表全部检查，并在完成后记录提交、run、日志与统计。

## 验证边界

本轮修复删除用户调用链的编译阻塞，以及由新增账号有效性检查暴露的旧测试数据遗漏。现有回归不替代新增用户管理全部业务专项验收，真实设备停网、离线/超时反馈、OIDC/旧会话拒绝及删除与并发写入等场景仍按原需求记录管理。

原始失败证据继续保留在 `process/test/2026-10-23-node-attributes-main-github-actions.md`。本轮没有部署服务或操作实际用户数据。
