# T28 · 新建/切换/克隆草稿 CAS 与多页冲突保护

> 状态：**待开发**；阶段：P09；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**集成负责人**。
- 文件边界：AutomationCenter.vue；automation-draft-sync.ts；automation-draft.ts；ScriptLibrary.vue 兼容接线。
- 依赖/领取条件：T20、T26、T27。

## 具体开发步骤
1. 将未保存/服务端未同步/已冲突/正式已发布状态明确分层，不因关闭浮窗丢数据。
2. 在新建、选中、克隆前保护旧草稿及正在编辑参数，允许保存并继续/留在当前/明确放弃。
3. 异步请求使用 owner/device/编辑 generation 与 revision 保护，网络未知不自动创建重复 ID。
4. 保持一份共享草稿与原 T10/T11 的 CAS，并发/多浏览器冲突不能强行覆盖。

## 完成判定
- [ ] 从 A 改动未同步时切 B 无静默覆盖，CAS 409 保留 A 并提示处理。
- [ ] 切账号/手机断线/浮窗关闭不泄露前账号脚本或抹掉服务器已确认步骤。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
只能集成负责人改共享接线；不能在 ScriptLibrary 和浮窗各自实现 draftSync 一份。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
