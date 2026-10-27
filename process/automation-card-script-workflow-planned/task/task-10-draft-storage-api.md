# T10 · 服务端草稿存储与CAS修订

- 里程碑：M3；负责人（唯一写入）：后端数据 Agent；相对工作量：L
- 依赖：T01；映射验收：AC-11 AC-12
- 当前状态：**代码已提交，待联调**；未执行本地测试、构建或真机验收，CI 结果另行核对。
- 写入范围：easytier-web/src/db/automation 新增 drafts.rs；相应迁移和 restful/automation 新增 drafts.rs

## 2026-10-27 实际交付

- 新增 SQLite 迁移 `m20261027_000025_automation_drafts.rs`，建立 `automation_drafts` 和永久写入回执 `automation_draft_mutations`，草稿与正式 `automation_scripts` 分离。账号外键和 owner-scoped 索引均在数据库层建立；降级不会无提示删除未发布的草稿。
- 新增 `db/automation/drafts.rs`，支持 `list_drafts/get_draft/save_draft/discard_draft`，沿用已有账号授权与 `BEGIN IMMEDIATE` 事务；新建返回 ID 和 revision=1，后续更新按 `owner_user_id+id+revision` CAS 更新，冲突返回 `draft_revision_changed` HTTP 409，不覆盖其他页面内容。
- 每个 POST/PUT/DELETE 必须提供 canonical UUID `request_id`。同一账号+操作者+UUID 相同内容重发，会从写入回执表返回仅包含 ID/修订号/操作结果的精简幂等回执（`request_replayed:true`），不再次创建/修改/删除；重放后需要 GET 获取最新正文，避免删除后仍在回执表长期保留步骤。相同 UUID 与不同内容冲突返回 `draft_request_conflict` HTTP 409；历史请求的回执删除草稿后仍保留，阻止请求重放重新创建。
- 草稿允许空数组 `steps:[]`；已有步骤继续走与正式脚本一致的 `validation::steps`（最多100步，字段白名单、唯一UUID、条件范围安全检查），并拒绝 `input_text.text` 的直接字面值输入，要求使用参数名，防止在草稿存储运行时文字/密码。
- 新增 `restful/automation/drafts.rs` 与 REST 路由 `/api/v1/automation/drafts`、`/api/v1/automation/drafts/{id}`，所有接口沿用已有 AuthSession、owner query、origin 和后台账号权限检查；草稿 ID 使用 UUID 校验，越权访问不返回其他账号草稿。
- 可供 T11 直接使用的请求字段、示例、错误合同见 `t10-draft-api-contract.md`。**T10 尚未改动任何前端**，录入一步自动保存、恢复和版本冲突 UI 属于 T11，不能标记 AC-11/AC-12 已通过。
- **待联调**：SQLite migration 升级、首次保存/空草稿/100步、权限拒绝、CAS 409、同 UUID 幂等重发和删除重放，均未实际执行。

## 交付范围
新建按owner隔离的非正式脚本草稿 API（get/list/upsert/discard）；draft_id、revision、request_id和CAS冲突控制；保存已确认步骤但禁止存运行时凭据。

## 完成定义（DoD）
跨重启读取同顺序/ID，权限越权403/404，冲突409不覆盖，重放幂等。

## 测试与交接
- 完成后记录变更文件、关键接口、异常分支、真实执行的测试与缺失测试。
- 交接给集成负责人验证与原卡片/脚本库闭环；不擅自修改其他 Agent 的唯一负责文件。
- 若实现依赖和实际源码不符，先更新任务卡并解释原因，不得绕过权限或设备独占协议。
