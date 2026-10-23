# feat/node-attributes-main GitHub Actions 测试报告

## 测试对象与授权

- 日期：2026-10-23（Asia/Shanghai）。
- 仓库：<https://github.com/CiBeDerORG/EasyTier>。
- 分支：`feat/node-attributes-main`。
- 用户授权：补跑本分支尚未完成的 GitHub Actions 测试。
- 本轮业务代码基线：`4e2ed4dd87461353a5ebea2786edf6793f2b48ea`，`feat(web): add user management and disable deleted users networks`。
- 实际测试提交：`9dc37dd0cebc32696c41cddc0b4fd75f3a435db3`，仅新增本轮专用工作流；业务代码与上述基线一致。
- 工作流：`EasyTier Web Regression`，`.github/workflows/node-attributes-validation.yml`。
- Actions 运行：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496>。
- 触发时间：2026-10-23 11:30:38（Asia/Shanghai）；事件：`push`。

## 为什么补充工作流

原 `.github/workflows/test.yml` 的自动触发分支不包含当前功能分支，且 nextest 只选择 `easytier` 与 `easytier-core`，未执行用户管理所在的 `easytier-web` 测试。本轮新增专用 Web 回归入口，两个 job 并行执行。

专用工作流的 push 条件同时限定为 `feat/node-attributes-main` 和该工作流文件本身。普通源码、需求或测试报告提交不会触发本工作流，保留调试模式下的既定协作规则。另保留 `workflow_dispatch` 手动入口。

本轮 CI 使用 GitHub 托管 Ubuntu runner，仓库权限仅 `contents: read`；未部署服务、发布制品、创建 PR 或操作实际用户数据。

## 验证范围与命令

| 验证项 | 命令 | 实际结果 |
| --- | --- | --- |
| Web 后端已有测试套件 | `cargo test --locked -p easytier-web --no-fail-fast` | 失败：编译错误 E0277，测试用例未开始执行 |
| 前端库已有单元测试 | `pnpm --dir easytier-web/frontend-lib test:config-ui` | 通过：10 个文件、57 个测试 |
| 前端类型检查及生产构建 | `pnpm --dir easytier-web/frontend build` | 步骤通过 |
| 网络配置映射回归 | `node easytier-web/frontend-lib/scripts/test-network-config.mjs` | 通过：10 组断言 |
| 节点属性页面浏览器回归 | 在 `easytier-web/frontend` 执行 `node --test tests/node-attributes.test.mjs` | 通过：13/13，0 失败、0 跳过、0 取消 |

工具链复用仓库配置：Rust `1.95`、protoc `35.1`、Node.js `22`、pnpm `10`；后端使用 `Cargo.lock`，前端使用 `--frozen-lockfile`。浏览器测试使用 runner 内安装的 Chromium，以及测试自行启动的本地 preview 和 mock API。

## 结果记录

**整体结论：失败。前端全部通过，后端在编译阶段失败，不能宣称本分支测试通过。**

本次运行于 2026-10-23 11:30:38 触发，11:35:27 完成（Asia/Shanghai），总历时约 4 分 49 秒。后端 job 耗时 4 分 45 秒，前端 job 于 11:32:17 成功完成，耗时 1 分 36 秒，其中 Chromium 浏览器测试约 12.08 秒。

### 后端阻塞

编译器在 `easytier-web/src/restful/user_admin.rs:145:51` 报告 `E0277`：删除用户接口函数 `delete_user` 不满足 Axum 的 `Handler<_, _>` trait 约束，因而不能注册为 DELETE 路由。

```text
error[E0277]: the trait bound `fn(..., ..., ..., ...) -> ... {delete_user}: Handler<_, _>` is not satisfied
  --> easytier-web/src/restful/user_admin.rs:145:51
145 | .route("/api/v1/admin/users/{id}", delete(delete_user))

error: could not compile `easytier-web` (bin "easytier-web" test) due to 1 previous error
Process completed with exit code 101.
```

依赖下载与 Rust/protoc 准备步骤成功。失败发生于项目自身编译阶段，并非 runner 排队、依赖下载或测试断言失败；本轮没有后端通过/失败用例数量可统计，历史的 339 项通过结果不能沿用。

- 后端 job：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496/job/120224536982>。
- 后端日志 artifact：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496/artifacts/12476642738>。
- 故障代码：<https://github.com/CiBeDerORG/EasyTier/blob/9dc37dd0cebc32696c41cddc0b4fd75f3a435db3/easytier-web/src/restful/user_admin.rs#L145>。
- 编译器建议使用 `#[axum::debug_handler]` 改善诊断；当前日志没有展开导致 trait 不匹配的具体内部类型，不能仅凭该消息断言某个锁或 future 是根因。
- 本轮完成测试执行和故障定位记录，未修改用户管理业务实现，也未把未执行的后端用例计为通过。

### 源码诊断与候选修复

只读追踪发现与该编译错误吻合的非 `Send` future 来源：

1. `delete_user` 等待 `disable_deleted_user_runtime`，后者进入 `ClientManager::stop_networks_for_deleted_user`。
2. `easytier-web/src/client_manager/mod.rs:348` 的 `timeout(..., async { ... })` 借用局部 `rpc`，并跨多个 `.await` 保留这一共享引用。
3. `easytier-web/src/client_manager/session.rs:991` 的 `SessionRpcClient` 是 `Box<dyn WebClientService<Controller = BaseController> + Send>`，没有 `Sync`；`easytier-proto/build/rpc.rs` 生成的 service trait 也没有 `Sync` supertrait。因此 `&SessionRpcClient` 不能保证 `Send`，与 Axum 对 handler future 的约束冲突。

最小候选修复是将上述局部 `async { ... }` 改为 `async move { ... }`，把满足 `Send` 的 `rpc` 所有权移入 future；该变量在 timeout 后不再使用。无需删除业务协调锁或改变超时、停网确认与并发策略。

这是基于源码的诊断。本轮没有应用该改动或重新编译，候选修复能否消除全部编译问题仍须实际验证。实际 CI 结论仍为上文记录的编译失败。

### 前端结果

- 前端 job：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496/job/120224536506>。
- 前端日志 artifact：<https://github.com/CiBeDerORG/EasyTier/actions/runs/39895399496/artifacts/12476024623>。
- 前端非阻塞提示：Browserslist 数据陈旧、PrimeVue 旧组件弃用、Vite 大 chunk 提醒及部分 Actions 的 Node 20 runtime 由 runner 转为 Node 24。相关步骤均成功，本轮未为这些提示扩大修改范围。
- 单元日志中的 `temporarily offline` 属于 WireGuard 设置读取失败后重试的既有测试路径；其所在测试文件全部通过，不属于 CI 故障。

## 覆盖边界与仍待验证的内容

本轮执行既有自动化测试与前端构建。新增用户管理代码目前没有相应的专项 Rust 测试模块或前端用例，因此即使这些回归通过，也不能把下列场景记为已完成验收：

1. `000014` 迁移对新旧真实数据库的兼容、用户逻辑删除幂等、自删及最后有效管理员保护、普通账号越权拒绝。
2. 删除后节点、实体设备、网关组、服务、网络定义及配置内容保留，全部持久实例禁用，其他账号不受影响。
3. 已删除账号登录、旧 Web 会话、OIDC 和 config-server 重连拒绝。
4. 删除与旧心跳、中央发布、配置保存、internal RPC 并发时，不能重新启动或重建网络。
5. 在线真实实例停止，离线或超时的未确认反馈，重复停止，以及账号删除已提交但运行停止失败的反馈。
6. 新用户管理页面的管理员菜单、确认、自删保护、普通用户直达拒绝、超时重试、权限失效及移动端交互。

本轮没有运行完整 core 网络内核集成测试、真实环境部署、`embed` 制品构建或上述专项人工验收。既有 ignored 压力测试保留 ignored，不计为通过。

## 相关记录

- 用户管理逐轮记录：`process/user-management-in-progress/task/test-report.md`。
- 上一轮实例绑定历史验证：`process/node-instance-binding-planned/task/verification-report.md`；其历史通过结果不替代本轮新增代码验证。
- CI 在各 job 的 `process/test/` 中保存日志和摘要，作为 `web-backend-test-report`、`web-frontend-test-report` artifact 上传，保留 14 天。
