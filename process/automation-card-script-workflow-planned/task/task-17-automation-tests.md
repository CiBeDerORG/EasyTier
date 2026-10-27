# T17 · 组件级和集成自动测试

- 里程碑：M5；负责人（唯一写入）：测试 Agent；相对工作量：L
- 依赖：T06 T08 T09 T11 T14 T15；映射验收：AC-01至AC-12
- 当前状态：**测试代码已提交，待完整自动验证及真机/浏览器验收**。增量 CI 首版发现一条测试断言错误并已修复；本轮不主动运行本地测试。
- 写入范围：`easytier-web/frontend/tests`、`automation-executor/tests/test_trial_handoff_t17.py`、`easytier-web/src/db/automation/drafts.rs` 下新增测试模块；现有 Node 单测入口及少量模块间 `.ts` 运行时导入。

## 交付范围
覆盖右键坐标和菜单、唯一匹配、重入幂等、替换ID保持、草稿CAS、脚本排他、无触控定位演示、跨设备授权与结果回步。

## 完成定义（DoD）
12 AC均有可追踪测试；负面和竞态检查有证据。

## 测试与交接
- 完成后记录变更文件、关键接口、异常分支、真实执行的测试与缺失测试。
- 交接给集成负责人验证与原卡片/脚本库闭环；不擅自修改其他 Agent 的唯一负责文件。
- 若实现依赖和实际源码不符，先更新任务卡并解释原因，不得绕过权限或设备独占协议。

## 2026-10-27 · 已提交的测试实现
- Node `test:unit`：新增 `automation-card-workflow-t17.test.mjs`，模拟真实视频像素与 UiAutomator 快照，验证拾取状态、旧 session 过期、重复确认幂等、四步追加、共享草稿投影、原位 CAS 编辑、只读定位高亮以及断开预览保留步骤。
- Node `test:unit`：新增 `automation-device-compatibility-t17.test.mjs`，验证跨手机坐标尺寸/方向不兼容必须阻断，同尺寸需人工确认；帐号、指纹、步骤正文、旧快照、XPath、非唯一选择器、控件树截断不会默默通过。
- Node `test:unit`：新增 `automation-run-safety-t17.test.mjs`，模拟 T13 预览移交与服务端二次只读确认；验证非本页会话、脚本占用、清理待确认、网络超时、账号变化均不擅自释放或执行；验证副作用确认和历史新旧脚本 revision 安全回修。
- Python `test_trial_handoff_t17.py`：使用真正的 SQLite Journal，不连接 Android/ADB；测试执行器手控与脚本双向排他、同一 ADB endpoint 不同资产不能并行控制、失败清理在重启后保持 cleanup_pending、其他手机并行独立。
- Rust `drafts.rs::t17_draft_regressions`：测试 UUID/CAS 修订输入、禁止保存字面输入密码、空草稿、幂等摘要以及 SQLite receipt 对 owner/actor/request/body 的隔离。
- `frontend/package.json` 已将三个 Node 测试文件纳入仓库既有 `test:unit`，不新增测试框架、不修改 CI 工作流。

## 测试状态（不能与提交状态混淆）
- GitHub Actions `40495290119` Web 阶段 `test:unit` 首版报告 **125 tests / 124 passed / 1 failed**，失败仅为 AC-12 断开预览测试对隐式 `undefined` 可选字段的对象全等比较；已在提交 `3d6b6d3` 修改为比较步骤 ID/类型/参数。该失败不证明业务数据丢失。
- 随后自动 CI 仍在运行，不能将此前通过的 124 项认定为修复后版本已全部通过；Rust 新用例和 Python 新用例均需另外核对实际调用情况。
- **未执行**本地测试、手动 GitHub workflow、手动 Rust/Python 编译及 FnOS/Android 真机和跨浏览器端到端验证（现有调试模式）。
- 完整测试矩阵见 `t17-coverage-matrix.md`；T18 仍负责用户真实场景逐项验收。

## 已知未覆盖
- WebRTC 浏览器事件真实右键/触屏长按、全屏弹层、UiAutomator2 真实安装和层级、服务端实际 HTTP 草稿 CAS 两浏览器并发、SPA 路由退出提示、FnOS/Android 实机执行与结果修复，需要浏览器+服务端/真机验收。
- Node 的纯函数断言、Python SQLite Journal、Rust helper 测试均不能替代这一层验收。
