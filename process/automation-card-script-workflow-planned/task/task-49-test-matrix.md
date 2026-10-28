# T49 · 后端自动化回归用例矩阵（代码已提交，尚未执行）

> 分支：`feat/automation-center`｜日期：2026-10-28｜测试状态：**未执行**｜运行权限：需用户明确授权  
> 本矩阵映射实际添加的 Rust + Executor Python 测试源码；**不构成测试通过报告**。

## Rust 主服务：真实迁移 SQLite 内存数据库

位置：`easytier-web/src/db/automation/tests.rs`（由 `mod.rs` 的 `#[cfg(test)] mod tests;` 注册）。

| 编号 | 测试函数（前缀 `t49_`） | 核心断言 | 当前结果 |
| --- | --- | --- | --- |
| RS01 | `empty_formal_script_can_be_persisted_but_is_not_an_executable_snapshot` | `saved_steps([])` 可保存、`steps([])` 拒绝 | 未执行 |
| RS02 | `update_requires_current_revision_and_preserves_newer_row_on_conflict` | 不匹配 revision 返回 409，原脚本未覆写 | 未执行 |
| RS03 | `revisioned_delete_never_erases_newer_revision_or_foreign_owner_script` | 旧 revision 409，其他账号 404，正确 revision 删除成功 | 未执行 |
| RS04 | `owner_scoping_and_actor_authorization_are_not_inferred_from_a_script_id` | 其他 owner 列表隔离、脚本更新/删除隔离、角色越权 403 | 未执行 |
| RS05 | `invalid_script_update_cannot_advance_revision_or_destroy_original` | 错误动作类型拒绝保存，原 revision/步骤保留 | 未执行 |
| RS06 | `foreign_actor_cannot_create_run_or_create_dispatch_rows` | 越权 actor 403，零任务入库 | 未执行 |
| RS07 | `reused_run_request_id_with_changed_body_fails_before_any_device_action` | 历史 request UUID 配不同正文返回 409 且旧 hash 不变 | 未执行 |
| RS08 | `empty_saved_script_is_rejected_by_real_create_run_even_with_a_valid_phone` | 正式创建手机资产/ADB 后，零步正式脚本 422，无运行记录 | 未执行 |
| RS09 | `run_uuid_is_idempotent_and_revision_is_checked_before_dispatch` | 首次 dispatch_pending；完全相同 UUID 不重复插入，不同内容 409 | 未执行 |
| RS10 | `rust_media_to_view_only_transition_fences_run_until_confirmed` | active/suspending 不可运行；确认 view_only 后允许建任务且原观看记录保留 | 未执行 |
| RS11 | `rust_run_dispatch_fences_manual_restore_and_new_media_admission` | 已建 dispatch_pending 时禁止恢复手控及再开新会话 | 未执行 |
| RS12 | `saved_run_rejects_stale_revision_before_persisting_any_action` | 旧正式修订 409 且无 run；正确修订正常建待派发记录 | 未执行 |

RS08～RS12 的手机是通过 `AssetsStore`、网络实例、phone 语义类型与 TCP ADB 服务 **真实业务 API 创建的数据库虚拟设备记录**；不会发 ADB、投屏、网络请求，也不代表已通过真机。

## Executor：真实 Runtime / SQLite Journal + 模拟 MediaManager

位置：`automation-executor/tests/test_view_only_lease_t49.py`。

| 编号 | 测试函数（前缀 `test_`） | 核心断言 | 当前结果 |
| --- | --- | --- | --- |
| EX01 | `viewer_releases_only_its_manual_write_lease_without_closing_media` | viewer 降权并释放自己写锁，媒体保持，手控命令受拒 | 未执行 |
| EX02 | `viewer_can_coexist_with_script_action_holder_but_cannot_resume_manual` | 观看/脚本写锁并存；写锁占用不能恢复手控 | 未执行 |
| EX03 | `unknown_manual_cleanup_never_becomes_confirmed_viewer` | 清理结果未知保持 cleanup_pending；不得升级确认 | 未执行 |
| EX04 | `viewer_close_does_not_release_a_different_script_holder` | 关闭观看不释放别人持有的 run 写锁 | 未执行 |
| EX05 | `foreign_action_holder_never_allows_a_viewer_takeover` | 别人拥有动作锁时 viewer 不能接管或发送控制 | 未执行 |
| EX06 | `other_executor_generation_cannot_downgrade_or_restore_viewer` | 错误 executor ID 不得降权/恢复手控 | 未执行 |

## 必须后续补充的真实验证

- Rust 实际编译、迁移版本与上述新测试运行结果、各用例日志或失败堆栈的**脱敏路径**。
- 双浏览器共享同一手机时，先观察后执行与重新手控的实际租约互斥；关闭页面/执行器异常重启后 cleanup_pending 是否阻断重放。
- FnOS 镜像 Digest 对应运行进程版本、Executor token 授权与反向代理/会话断开恢复。
- 运行中查看请求 UUID、设备指纹、步骤 ID 与会话 ID；任何异常不能用再次手动启动来替代历史只读核验。

**提交 ≠ 编译成功 ≠ 单测通过 ≠ 真机验收通过。** 不得将本文件任何“未执行”改为“通过”，除非实际运行并归档对应证据。
