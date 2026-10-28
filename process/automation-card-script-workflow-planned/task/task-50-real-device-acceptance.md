# T50 · FnOS + 双安卓品牌 + 双浏览器全流程验收

> 状态：**待开发**｜计划：P16｜复杂度：L｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**QA 真机 Agent**。
- 文件边界：task/acceptance-report.md + 脱敏录屏证据引用；不修改生产代码。
- 开始条件：T48、T49；所有 FINAL 执行代码完成且取得用户测试授权。

## 实现步骤
1. 在 FnOS 用实际镜像 Digest 和至少两种 OEM Android 的真实 ADB/UiAutomator2 手机验收。
2. 逐步演示打开原视频→自动化 icon→可拖主子浮窗→全屏切换→新建/选择/克隆→拾取四步→逐步编辑删除。
3. 测试单步 ▶、整套 ▶、失败修复、双浏览器争用、断线、切 owner、流重连、过期快照、坐标尺寸不匹配。
4. 使用请求 UUID/runId/stepId/video sessionId 和录屏观察“原视频不主动关闭”的证据。

## 通过标准
- [ ] 逐条记录 FINAL V3 FF01～FF18 + 原 AC01～AC12 的实际结果、失败步骤、重现与证据。
- [ ] 高风险动作按指定测试手机与授权范围执行，不在未知结果自动重试。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
此任务必须获得用户明确的真实操作/测试授权；不能从规划自动推断已允许。

## 本任务当前状态
只建立任务计划；未修改功能代码，未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T50 测试前置矩阵已创建，未开始真机动作

- 已创建 [T50 实际验收与取证矩阵](task-50-acceptance-matrix.md)：严格引用 FINAL V3 FF-01～FF-18 + 原 AC-01～AC-12，共 30 条，全部标记**未执行**。
- 按可复现证据定义：完整 Git SHA、FnOS/Android OEM/浏览器版本、主服务与 Executor 镜像 digest、媒体 session ID、脚本 run/step/request ID、owner/设备指纹脱敏标识、录像和失败复测。
- 按危险度安排先只读后真实动作，未知执行结果只读核对、不重发；用户未明确授权 FnOS/手机真实动作前，不实际开始此任务，也不声称通过。
- 后端自动验证交接见 [T49 测试矩阵](task-49-test-matrix.md)；目前 Rust/Python 自动用例也未执行。
