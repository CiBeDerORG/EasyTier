# 自动化中心 push 自动构建

2026-10-24：当前交付分支为 `feat/automation-center`。现有镜像工作流已配置该分支的 push 触发、构建任务条件和独立镜像标签，无需重复修改 YAML。本轮将用户要求写入协作规则，后续代码 push 不默认使用跳过 CI 标记。

当前阻塞：GitHub Actions 页面明确显示该 fork 因 Actions 使用规模被暂停，需要维护者重新启用。当前环境无法访问 GitHub API，也没有可用的已登录网页操作能力，尚未恢复执行或发布新镜像。

入口：[需求](require/README.md)、[计划](plan/README.md)、[执行记录](task/test-report.md)。
