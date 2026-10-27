# T32 · 步骤编辑、重新拾取、只读演示浮窗化

> 状态：**待开发**｜计划：P10｜复杂度：M｜交付分支：`feat/automation-center`｜总合同：`../require/require-05-final-card-script-workflow.md`

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
