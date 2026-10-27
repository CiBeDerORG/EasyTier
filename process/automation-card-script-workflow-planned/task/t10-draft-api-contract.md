# T10 → T11 服务端草稿 API 合同（2026-10-27）

所有请求：登录会话 Cookie；`?owner_user_id=<正整数>`；跨账号操作只允许现有管理员授权。响应沿用现有 Automation API JSON / `{code,message,field_errors}` 错误结构。

## Endpoints

| 方法 | 地址 | 输入 | 返回 |
|---|---|---|---|
| GET | `/api/v1/automation/drafts` | 仅 owner 查询 | `{items:Draft[]}` |
| GET | `/api/v1/automation/drafts/{draft_id}` | 仅 owner 查询 | `Draft` |
| POST | `/api/v1/automation/drafts` | `{request_id,name,description,steps}` | 首次：`Draft`（revision=1）；重放：精简回执 |
| PUT | `/api/v1/automation/drafts/{draft_id}` | `{request_id,revision,name,description,steps}` | 首次：`Draft`（CAS 旧 revision+1）；重放：精简回执 |
| DELETE | `/api/v1/automation/drafts/{draft_id}` | JSON body `{request_id,revision}` | 首次：`{id,deleted:true,revision}`；重放：精简回执 |

`Draft` 含 `id,owner_user_id,name,description,steps,revision,created_at,updated_at`。可为空步骤 `steps:[]`，但存在步骤时必须是现有正式脚本验证支持的完整结构；步骤 ID 保持 canonical UUID，最多100步。

**不接受运行时参数值**：`input_text` 只能保存如 `{"type":"input_text","id":"<uuid>","parameter":"account_name","selector":{"resource_id":"com.example:id/account"}}` 等已验证步骤结构，不能提供 `text` 字面值；用户输入的真实文字仅在试跑/运行时按现有 executor 协议提供，不写入草稿。不要把登录密码填进 `name` / `description` / 定位文字。

## T11 接入顺序

1. 用 `GET drafts` 列出用户已有草稿并选择恢复，或 `POST drafts` 为当前活跃草稿创建独立 ID。
2. 每次确认步骤后，使用**同一个草稿 ID** 按原 `revision` PUT 完整步骤数组；接收返回的 `revision` 再发送下一次保存。下一次更新必须等待前一个写入返回，不能并行覆盖。
3. 对**相同的一次网络重试**必须复用同一 `request_id` 和完全相同的 body。不同的业务修改必须重新生成 request_id（否则 409）。
4. 重放识别：如果返回 `request_replayed:true`，响应仅含 `id,revision,deleted,request_replayed`，不含草稿正文。PUT/POST 重放后用 GET 获取最新完整草稿（可能比回执更新）；DELETE 重放不 GET 删除目标。
5. 收到 `draft_revision_changed` HTTP 409 时，禁止重写服务端步骤；先 GET 获取对方最新版，允许用户查看差异、另存或人工合并。
6. 收到网络超时等未知结果时先使用同一 request_id 重试；服务端仅保存回执元数据和请求哈希，已删除草稿的正文不会遗留在回执表。重试可能返回旧 revision，需要额外 GET 最新数据。
7. 显示四态：`尚未保存到服务器`／`保存中`／`已同步·修订 N`／`未同步或版本冲突`，勿把前端内存写入冒充已同步。用户确认输入尚未落地时阻止静默丢弃。
8. 保存正式脚本时仍调用现有 `/scripts` API，不能把草稿记录直接当正式可运行脚本；保留两类资源不同的生命周期。

## 错误语义

- `403 automation_access_denied`：账号权限不足，后端同时校验 actor/owner；不会展示他人草稿。
- `404 automation_not_found`：目标草稿不属于该账号或已删除，避免枚举其他账号的 ID。
- `409 draft_revision_changed`：另一个页面先修改/删除；本次绝不覆盖。
- `409 draft_request_conflict`：同一个 request_id 用于不同操作、目标或内容；本次不修改。
- `422`：字段、UUID、步骤结构或敏感字面输入非法。
- `500` / 网络错误：状态未知，沿原 request_id 进行重试，或 GET 确认是否持久化。

## 待执行验收

- SQLite 迁移前后数据安全，空草稿/步骤顺序保留/后端重启可读。
- A/B 标签页同时读取 revision=1，A 更新成功至2，B 更新返回 409；B 获取最新版后修订3可以成功。
- 两次相同 UUID 的 POST 不产生两份草稿；两次相同 UUID 的 PUT 不多加 revision；同 UUID 不同 body 返回409；已删除草稿的同 UUID DELETE 重放只返回历史结果。
- 非所属账号的 GET/PUT/DELETE 不泄漏草稿；旧会话及无效管理员无权限操作。
- `input_text.text` 拒绝保存；仅参数名步骤保存成功；无执行器就绪也能保存草稿。
