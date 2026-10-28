# T48 · 浮窗交互、脚本 CAS 与异常前端回归测试

> 状态：**回执/身份/浮窗堆叠单测已提交，浏览器交互和执行仍待授权**｜计划：P16｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**QA 前端 Agent**。
- 文件边界：frontend 对应自动化单测新文件（只写测试，不碰生产集成文件）。
- 开始条件：T21～T35、T40～T45 相应功能提交后。

## 实现步骤
1. 覆盖图标布局、menuOpen 与面板打开独立、全屏 Teleport、触摸拖拽误触隔离、Esc/dirty 状态。
2. 覆盖空脚本、切换、克隆、新 scriptId、每步删除/条件块、步骤编辑与 CAS 409。
3. 覆盖单步/整套 runId/requestId 回执不丢和失败/未知不得重发；UI 定位演示只读。

## 通过标准
- [ ] 针对全部新增交互提供回归测试与结果记录；有断言而非仅渲染快照。
- [ ] 既有 T17 125/125 的历史结果不冒充本次测试通过。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
创建测试代码不等于获准运行测试；按仓库调试模式等待用户后续明确授权。

## 本任务当前状态
已新增第一批回归测试源代码并接入单测清单；当前调试模式下未运行测试、检查、构建或真机操作。

## 2026-10-28 · T48 第一批（仅编写，不执行）

- `frontend/tests/automation-inline-batch-regression.test.mjs` 新增 5 组显式断言：批量 `requestId` 只能保留/清除同一个 ID、账号隔离、旧 UUID 未核实不得产生第二个提交、损坏/缺字段的持久凭据必须拒绝、存储只含请求及设备元数据不含明文参数/步骤、单步冻结源选择器且 owner/device/fingerprint/step 修改使其失效、只有兼容的只读无参数动作才可以自动提交。
- `frontend/package.json` 将新增文件纳入既有 `test:unit` 测试清单。仅更新脚本列表，不触发运行，也不更改 GitHub Actions 工作流。
- **未执行** node --test、TypeScript、lint、Vite、Runner/CI、浏览器/Android 或 FnOS 实测，尚不能报告断言通过数量。UI 的拖拽、全屏 Teleport、ESC/dirty、执行历史恢复、动态手机选择仍待后续 T48 用例或真实联调覆盖。

## 2026-10-28 · 第一批断言补充
- 新增独立批量回执校验反例：错误 UUID 格式、重复/空目标 ID、21 台越界、无效日期均必须在保存前拒绝，存储值保持空；其余冻结单步骤动作与未知提交保护断言详见同日第一批记录。
- 回执模块因断言设计补充了严格格式与唯一设备约束。所有代码**仅提交、未执行**；后续仍应补前端可交互浮窗、ESC/全屏、CAS 409、读历史未知状态。

## 2026-10-28 · T48 第二批 · 子窗排序 / 共享正式回执 / 手机身份

- 新增 `frontend/tests/automation-floating-dialog-order-t48.test.mjs`：直接调用实际 `useAutomationFloatingDialogOrder` 组合式函数，覆盖顶窗可交互资格、raise 顺序、关闭释放后另一子窗恢复、重复注销不抢占另一窗口。它属于无 DOM 的逻辑回归，**不等于**完成 Esc 键、Fullscreen/Teleport、触摸、拖拽浏览器测试。
- 把原来写在 `automation-script-workspace.ts` 内的正式脚本操作保护提取为 `automation-formal-script-receipt.ts`，存储结构/键名保持兼容，只保存 owner、类型、脚本 ID、修订、期望名称与 step IDs、日期；对请求身份、新建 UUID、时间戳、重复 step ID 做检查。对调用方对象进行 allowlist 序列化，明确不持久化定位器、输入文字或脚本正文。损坏/不可读取的浏览器凭据阻断新发布/删除/切换，UI 入口不再直接抛异常。
- 新增 `frontend/tests/automation-formal-script-receipt-t48.test.mjs`：覆盖新建 revision 0 与临时 ID、已保存 PUT/DELETE、写后恢复、跨账号隔离、坏 JSON/重复 ID/时间戳/版本拒绝、不同回执无法清除、敏感额外字段绝不落盘、存储失败禁止发送。
- 新增 `frontend/tests/automation-floating-scope-t48.test.mjs`：直接覆盖 `automation-floating-contract.ts` 原 PhoneCard owner/device/target fingerprint、脚本 revision、草稿 revision、generation 漂移拒绝，允许离线只改草稿但媒体操作必须保持同一原 session。
- 上述三个新测试已加入 `frontend/package.json` 的 `test:unit` 清单；此前的批量 UUID/单步冻结测试仍保留。
- **状态**：仅编写/提交，未执行 Node/Vue/TS 测试、浏览器 E2E、类型检查、构建或手工 CI。FullScreen Teleport、按键/触摸、跨浏览器实际竞争及 CAS 409 仍需补覆盖和 T50 联调。
