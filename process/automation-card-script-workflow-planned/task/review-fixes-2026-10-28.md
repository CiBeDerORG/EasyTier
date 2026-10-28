# FINAL V3 · 2026-10-28 自动化中心静态评审修复

基线：`feat/automation-center` / `9feee9bbd05678bf7c89730c6cfc21ded1357ab9`。
范围：P1-01～P1-04、P2-01～P2-02。此次用户明确要求开始修改；
遵循既有调试模式，只改源码、编写自动测试和更新文档；未手工运行任何测试、CI、构建或实体手机。

## 已提交源码（等待新代码版本自动 CI 复核）

| 编号 | 修复 | 关键文件 | 尚需确认 |
| --- | --- | --- | --- |
| P1-01 | batch 生命周期仍保持 `completed`，UI 顶部颜色和文案改用原始 `run.state`，区分成功、失败、部分失败、取消、中断 | `FloatingBatchRunPanel.vue` | 自动浏览器渲染、真实失败返回 |
| P1-02 | `unknown` 增加原视频保护，阻止未确认运行期间筛选/关闭画面，原视频会话未改变 | `useFloatingBatchRun.ts`、`PhoneCard.vue` | 页面真实 WebRTC 及清理 |
| P1-03 | 追加 SeaORM migration 029：持久的 `(owner_user_id,request_id)` 创建回执/哈希/响应，`BEGIN IMMEDIATE` 下对同 UUID+同正文返原结果，不同正文 409；即使脚本已删也保留墓碑；POST 必填 UUID，GET 精确只读恢复；新建/克隆/首次正式发布都先保存标签页 UUID | `migrator/*029*`、`db/automation/scripts.rs`、`restful/automation/scripts.rs`、`automation-script-workspace.ts` 等 | SQLite 升级、旧无 UUID 调用兼容（现返回 422）、真实并发提交 |
| P1-04 | 旧不带 revision 的 `DELETE /scripts/{id}` 仍匹配路由但固定拒绝写入（409）；安全删除必须使用 `POST /scripts/{id}/safe-delete`，避免旧客户端绕过 CAS | `restful/automation/scripts.rs` | 旧客户端可能需要升级后删除 |
| P2-01 | 仅当批量未提交且明确 `failed`，逐机核对原 session/owner/target，经过 Rust+Executor 安全恢复后复核，提供失败原因；运行中/未知禁止该捷径 | `useFloatingBatchRun.ts`、`FloatingBatchRunPanel.vue`、`PhoneCard.vue`、`AutomationCenter.vue` | 未实际验证恢复后触控/并发清理 |
| P2-02 | 新建无 Android 的 Chromium 夹具，直接挂载生产 `PhoneCard.vue` + 主/子浮窗/步骤编辑；验证主视频 DOM 身份在浮窗/全屏不被替换、`unknown` 保护开关 | `tests/fixtures/automation-phone-card-review.*`、`tests/automation-floating-browser-t48.test.mjs` | 仅模拟宿主，不是实际 WebRTC/手机；自动浏览器 CI 待出结果 |

## 新增自动化回归

- Rust `db/automation/tests.rs::review_create_uuid_replay_returns_same_formal_script_and_refuses_different_body`：真实 in-memory 迁移、同 UUID 重放、不同正文冲突、账号隔离、正式脚本删除后的回执墓碑。
- Chromium `T52 actual PhoneCard...`、`T52 original PhoneCard...`：挂载真正 Vue SFC 而非占位屏幕，无网络/ADB/手机动作。
- 测试**编写完成，但本轮未手工执行**。上一次已通过的 2026-10-28 GHCR 构建（源代码 `9120b252`）不能代表本轮修复后的代码已通过。

## 限制与残余风险

1. 改动旧创建/删除请求合同：外部旧调用方没有 `request_id` 或 revision 的应升级，不能静默退化为不安全写入。
2. 浏览器模拟无法代替真正 WebRTC 在 FnOS/Android 环境下的全屏、ADB 和动作隔离验收。
3. 正式交付前检查**本次最终源码对应**的自动 CI 是否真正成功；任何失败均须单独修复。
4. 本轮无真机测试或发布环境部署；原 T50、T51 最终真机签字仍不标完成。
