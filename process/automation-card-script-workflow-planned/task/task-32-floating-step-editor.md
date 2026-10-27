# T32 · 步骤编辑、重新拾取、只读演示浮窗化

> 状态：**前端业务代码已提交，待验证**｜计划：P10｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

## 唯一职责与依赖
- Owner：**步骤 Agent**。
- 文件边界：CardStepEditor.vue；复用 ScriptStepEditor.vue；AutomationFloatingDialog.vue 由浮窗 Agent 提供。
- 开始条件：T24、T29、T20。

## 实施清单
1. 迁移卡片内嵌 CardStepEditor 至可拖动子窗，继续调用原 updateCardDraftStep 期望值冲突判断。
2. 保留编辑参数、定位演示、重新拾取、保存、放弃未保存操作和当前 stepId。
3. 保存只修改本步骤，不发送手机动作；演示只读 GET，旧控件树/设备绑定变化时失效。

## 验收条件
- [ ] 编辑 1/2/3/4 步不会误新建步骤；当前手机原视频始终可见。
- [ ] 进入全屏后可输入、拖动、保存或取消，未保存状态正确保留。
- [ ] 交接时列出代码范围、状态协议、回归风险、未完成项；单独 push 不表示实机验收。

## 保留与禁止
编辑器不自建第二个 video 或第二份草稿；共用现有表单校验。

## 验证状态
当前仅有计划文档；待开发、未测试、未构建。遵守仓库 AGENTS.md 调试模式，不主动运行本地检查或手动 CI。

## 2026-10-27 · T32 开发记录
- 旧 CardStepEditor 的同一 ScriptStepEditor 参数表单，已放入 AutomationFloatingDialog 且挂载到原 phone-screen 的子窗口层；全屏不另开视频。
- 复用原 updateCardDraftStep 期望值比对、重新拾取、只读定位演示、取消和保存；浮动关闭与编辑切步对 dirty 使用显式保护。
- 已删除卡片 footer 的重复编辑外壳，仅在主浮窗展示步骤列表；旧 CardTrialPanel 模态试跑与独立 Inspector 仍等待后续任务替换，不提前删除。
- **未执行** TypeScript、lint、单测、构建、手动 CI、浏览器/Android/FnOS 实测。完成判定仍待验证，不能以提交代替真实验收。
