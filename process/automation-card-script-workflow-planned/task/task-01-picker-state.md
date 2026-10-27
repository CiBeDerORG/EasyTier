# T01 · 拾取工作区状态合同

- 里程碑：M0；负责人（唯一写入）：集成负责人；相对工作量：M
- 依赖：无；映射验收：AC-01 AC-07 AC-12
- 当前状态：**待联调**（代码已提交，未测试）；未执行测试/构建。
- 写入范围：AutomationCenter.vue；新建 automation-picker-state.ts

## 实际交付（2026-10-27）
- 新增 `automation-picker-state.ts`：`PickerPhase`、`PickerIntent`（append/replace）、设备绑定 + 会话 ID + 草稿脚本/修订、单调 `generation`、树快照 `snapshotGeneration`、异步结果 `pickerCanApply` 校验。
- `AutomationCenter.vue` 统一持有每机 `pickerContexts`，旧 Inspector 入口复用一份 `draftWorkspace`；关闭会话、切换账号/页面、设备解绑、开始录制/群控、视频方向改变时进行状态失效或快照版本失效，不删除已确认步骤。
- **T02/T03 尚未做**：卡片图标仍沿用旧 Inspector 入口，尚无可视拾取开关或右键菜单；未来只需订阅每机 phase 并触发 arm/stop。
- **T04/T06 尚未做**：旧 Inspector 不具备右键触发时的 generation 快照 token 回传；目前只完成基础条件保护。异步防重入需要在后续任务传递捕获 token。
- 现存风险：当前草稿为内存态，网页刷新/退出时不能保证恢复，交 T10/T11。

## 交付范围
为每台卡片管理拾取开关/append-vs-replace/current-draft/snapshot generation；账号、会话、设备变化安全失效；规定 props/events 接口。

## 完成定义（DoD）
同一设备状态独立，草稿不复制；断开画面模式关闭、步骤不删除；与录制/群控互斥。

## 测试与交接
- 完成后记录变更文件、关键接口、异常分支、真实执行的测试与缺失测试。
- 交接给集成负责人验证与原卡片/脚本库闭环；不擅自修改其他 Agent 的唯一负责文件。
- 若实现依赖和实际源码不符，先更新任务卡并解释原因，不得绕过权限或设备独占协议。
