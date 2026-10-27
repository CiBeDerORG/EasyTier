# T22 · 悬浮 icon 重新排位与开关

> 状态：**待开发**；阶段：P08；难度：S；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**卡片 Agent**。
- 文件边界：easytier-web/frontend/src/components/automation/PhoneCard.vue（唯一写入）。
- 依赖/领取条件：T21。

## 具体开发步骤
1. 原 side-actions 的唤醒按钮位置改为自动化 icon，唤醒 icon 上移一格。
2. 自动化按钮一击打开浮窗，再击关闭；按钮有明确 aria-label、激活态、键盘支持。
3. 保留原关闭投屏/全屏/录制/返回/最近/主页/手动唤醒操作和现有 dock 左右停靠。

## 完成判定
- [ ] 按钮位置符合用户指定，唤醒原命令 WAKE 未变化。
- [ ] 自动化按钮不受 manualAllowed 禁手控而无法打开查看。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
只改本手机局部菜单；禁止把全局批量脚本执行绑定到这个开关图标。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
