# T43 · 浮窗全局运行入口与不可变任务提交

> 状态：**工作台批量完整脚本主链路已提交，未联调验收**｜计划：P14｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**运行 Agent + 集成负责人串行接线**。
- 文件边界：新 automation/useFloatingBatchRun.ts、AutomationScriptPanel.vue；AutomationCenter.vue 由集成负责人。
- 开始条件：T28、T39。

## 实现步骤
1. 主浮窗提供“运行全流程”，目标只来自工作台勾选且授权的手机；0 台明确阻止。
2. 获取当前脚本正式 revision 或已确认草稿 snapshot，冻结步骤/参数名/设备列表/fingerprint/request UUID。
3. 复用现有 ScriptLibrary.prepareRun 与 runs API，避免额外 run dialog 另开预览；不能把单步执行绑定到勾选全集。

## 通过标准
- [ ] 只运行所选手机，连续点按钮或超时不产生重复运行记录。
- [ ] 0 步或未授权脚本禁止执行，后续编辑不修改在途快照。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
先接上 T39 安全写锁移交，不能仅隐藏旧试跑 modal 来绕开排他。

## 本任务当前状态
已提交代码，仍未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T43 代码交付
- `frontend/src/modules/useFloatingBatchRun.ts` 冻结 owner、所勾选 deviceIds、单机 fingerprint、不可变完整 steps 快照、参数及一次性 UUID；0 台、空步骤、无效条件结构、未授权/失联设备硬阻断。只用 `POST /runs` 一次提交所选设备，绝不按照手机逐台重复 POST。
- `automation-batch-receipt.ts` 在有风险 POST 前使用 `sessionStorage` 仅持久化 requestId、设备ID及时间；HTTP 结果未知后只能通过 `GET /runs` 历史只读核实，列表未找到不当成动作未发生，不能自动创建第二个 UUID。
- `FloatingBatchRunPanel.vue` 挂在每台原手机的脚本主浮窗里，共用工作台选中设备与唯一共享草稿，不切 `ScriptLibrary` 页面、不另开 video。参数输入为本次内存值，不写持久凭据。
- `AutomationCenter.vue` 仅当服务端草稿同步 guard 为 clean、无编辑/录制/群控/解锁冲突时允许启动；同一批任务后续勾选变化不影响冻结目标。逐台原画面 SessionId 确认 Viewer 只读降权和 UiAutomator2 准备成功后再次从服务器核对所有设备，才整批提交。
- 当前 Rust `/runs` 一次请求仍有 20 台硬限制；新界面如实限制，不隐式拆单；若需要每批不限台数，应另改结果校验、Rust DB/Executor request 大小和运行事件上限。
- **状态：代码已提交，未做 TS/Rust/Python 检查、单测、构建、CI、浏览器/真机联调；任务验收框不勾选。**
