# T10 · 服务端草稿存储与CAS修订

- 里程碑：M3；负责人（唯一写入）：后端数据 Agent；相对工作量：L
- 依赖：T01；映射验收：AC-11 AC-12
- 当前状态：**待开发**；未执行测试/构建。
- 写入范围：easytier-web/src/db/automation 新增 drafts.rs；相应迁移和 restful/automation 新增 drafts.rs

## 交付范围
新建按owner隔离的非正式脚本草稿 API（get/list/upsert/discard）；draft_id、revision、request_id和CAS冲突控制；保存已确认步骤但禁止存运行时凭据。

## 完成定义（DoD）
跨重启读取同顺序/ID，权限越权403/404，冲突409不覆盖，重放幂等。

## 测试与交接
- 完成后记录变更文件、关键接口、异常分支、真实执行的测试与缺失测试。
- 交接给集成负责人验证与原卡片/脚本库闭环；不擅自修改其他 Agent 的唯一负责文件。
- 若实现依赖和实际源码不符，先更新任务卡并解释原因，不得绕过权限或设备独占协议。
