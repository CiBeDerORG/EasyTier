# T37 · 执行器媒体观看权与动作写锁分离

> 状态：**待开发**｜计划：P12｜复杂度：L｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。
