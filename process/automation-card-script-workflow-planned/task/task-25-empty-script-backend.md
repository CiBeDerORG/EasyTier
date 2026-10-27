# T25 · 空白脚本 0 步持久化契约与后端改造

> 状态：**待开发**；阶段：P09；难度：M；分支：`feat/automation-center`；产品源：`../require/require-05-final-card-script-workflow.md`。

## 唯一写入责任
- 负责人：**Rust 脚本数据 Agent**。
- 文件边界：easytier-web/src/db/automation/validation.rs、scripts.rs；restful/automation/scripts.rs（若必要）。
- 依赖/领取条件：T20（接口冻结）。

## 具体开发步骤
1. 确认 Rust validate steps 现仅接受 1–100 步，分开“正式脚本可保存为空”与“运行脚本必须有 1–100 有效步骤”的不同限制。
2. 支持 POST scripts 创建 0 步带名称的真实服务端记录；读取/更新、revision、owner 授权、步数上限、旧数据兼容。
3. 保留执行前验证，绝不让 0 步脚本发 create_run 或绕过成功断言。

## 完成判定
- [ ] 创建 0 步脚本后 GET/list 可查且 revision 明确；0 步 run 应被拒绝。
- [ ] 其它非法步骤、条件分支、不允许的敏感字段仍按当前校验阻断。
- [ ] 交接时提供修改文件、API/event 契约、已知风险、待联调场景；仅代码提交不等于真机验收。

## 禁止与风险
如果业务层需要另建 Draft 记录，请先按 T20 契约区分，不得用前端假成功代替真实数据。

## 交接/验证状态
- 未开始、未实施、未测试；当前仓库调试模式未经后续授权不执行本地测试、构建或手动 CI。
