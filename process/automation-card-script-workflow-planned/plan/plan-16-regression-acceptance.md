# P16 · 自动化测试、FnOS 真机及最终验收

> **状态：待开发**｜**FINAL V3 计划**｜**目标分支：** `feat/automation-center`｜**产品主合同：** `../require/require-05-final-card-script-workflow.md`。

## 业务目标
从 DOM/纯逻辑、Rust/Executor 状态，再到 FnOS+双 OEM 手机+双浏览器真实全屏执行形成可证据链，验收所有 FINAL V3 FF01～FF18 和原 AC01～AC12。

## 前置条件与关联任务
- 前置：逐阶段跟随 P07～P15；终验依赖全部实现。
- 开发任务：[T48](../task/task-48-frontend-regression.md)、[T49](../task/task-49-backend-regression.md)、[T50](../task/task-50-real-device-acceptance.md)、[T51](../task/task-51-final-acceptance-report.md)。

## 交付要求
1. 前端浮窗、拖动边界、全屏 portal、点击隔离、CAS/克隆、删除分支、单步回执和并行 UI 单测。
2. Rust/Executor owner/fingerprint/session/lease、异常清理、幂等未知结果/跨浏览器抢占自动测试。
3. FnOS + 两个 Android 品牌 + 双浏览器手工操作与录屏，完整的脚本新建→拾取四步→编辑克隆→单步→全局→失败修复。
4. 将每条 FF/AC 结果、版本 SHA、镜像 Digest、run/step IDs、环境、复现与未通过缺陷写入验收报告。

## 可验收判据
- [ ] A 类功能代码/自动测试/实机记录分开标，不以历史 125/125 代替新增功能测试。
- [ ] 所有强制业务验收通过且真实证据齐全，才标 FINAL 完成；否则逐项列未完成和修复责任人。

## 边界与风险
项目处于调试模式，创建计划不授权执行测试或手动 CI；依照仓库 AGENTS.md 取得用户后续明确的测试授权。

## 禁止提前声称完成
当前仅完成需求与任务拆分；无新增业务功能验收证据时状态必须维持“待开发/待联调”，不把文件创建计作交付。

## 2026-10-28 · T49 Rust + Executor 测试源码完备、尚未执行

- `easytier-web/src/db/automation/tests.rs`（`#[cfg(test)]` 注册）包含 12 项基于真实迁移 SQLite 内存库的正式脚本 CAS、空脚本保存/运行拒绝、跨账号授权、请求 UUID 幂等、二阶段 viewer/action 租约测试。
- 合法手机环境全部来自 `AssetsStore` 真实资产 API + phone 语义类型 + TCP ADB 服务模拟 DB 记录，不发真实安卓指令。
- `automation-executor/tests/test_view_only_lease_t49.py` 的 6 项测试使用真实 Runtime/Journal 并模拟媒体边界，关注 Viewer 与 action holder 不抢占、清理失败不被错误确认。
- 所有条目仍待实际 Rust/Python 运行验证；真实 WebRTC、FnOS、安卓与双浏览器属于后续 T50。完整交接见 [T49 测试矩阵](../task/task-49-test-matrix.md)。

## 2026-10-28 · T50 真实验收前置材料

已准备 [T50 30 项验收与取证矩阵](../task/task-50-acceptance-matrix.md)，分别对应 18 项 FINAL V3 FF 与 12 项历史 AC，附 FnOS + 双 OEM + 双浏览器、镜像 digest、run/step/request/session ID、脱敏日志及中止规则。**尚未获得真机测试授权，未执行**；P16 整体仍处于待联调/待验收。
