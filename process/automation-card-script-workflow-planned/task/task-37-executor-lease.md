# T37 · 执行器媒体观看权与动作写锁分离

> 状态：**状态：业务代码已提交，待端到端联调验收**｜计划：P12｜复杂度：L｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**Executor Agent**。
- 文件边界：automation-executor/automation_executor/journal.py、runtime.py、media.py（独占 Executor 写入）。
- 开始条件：T36。

## 实施清单
1. 把现有运行/控制期间的读屏观看状态与 claim 的独占动作写 holder 分离，不扩大匿名或跨 owner 观看权限。
2. manual → prepare → script 控制权交接包含停手/未完成触点清理；执行动作前验证当前 lease/fingerprint。
3. 准备、脚本执行/结束/异常恢复都不主动关闭同一视频 session；session 视频异常显式报告断流。
4. 防双浏览器重叠命令、运行时手动 tap/input/unlock 绕行、重启残留锁/清理未知和重复动作。

## 验收条件
- [ ] 证明同设备脚本执行期间可以授权观看实时画面但所有非 holder 写命令被拒。
- [ ] 独占锁未知/清理异常 fail closed，不自动放开手控或自动重试动作。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
严禁复制第二套 scrcpy 会话来规避旧媒体约束；保留已有安全审计。

## 验证状态
计划阶段描述已由下方实际交付记录补充；代码已提交、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-28 · T37 实际代码交付
- `automation-executor/automation_executor/journal.py`：同 holder 的重入要求 kind/fingerprint/endpoint/device 均匹配，否则拒绝；原子独占 Journal 不删除。
- `runtime.py`、`app.py`：`sessions/{sid}/view-only` 显式单向降权并确认触摸释放与 worker 清理；`resume-manual` 要求执行器身份、当前媒体有效及无其它 holder；同一媒体会话与 WebRTC SDP 原样保持。只读拒绝 touch/key/text/stop/release/unlock；STOP/RELEASE 不能绕过其它 holder 的排他锁；未持有手控锁的释放操作是无副作用的确认。
- 修复清理失败后的降权重试误报成功：只有 `view_only_confirmed` 真正完成旧写锁回收，才可幂等成功；其余维持拒写/待确认。变更包含 `6d6620d`、`bad8722`、`369e4c4`、`4098a00` 等提交。
- **待验证**：手控输入执行中降权、已发送但回执未知、旧 worker cleanup_pending、脚本期间 STOP/解锁等硬拒绝、Executor 重启及旧镜像版本不支持时的 fail-closed 行为。
- **未执行**单测、Python 检查、构建、CI、浏览器或 FnOS/Android 真机验收（调试模式）。
