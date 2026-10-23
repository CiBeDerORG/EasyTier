# 服务公开网络选择修复验证

2026-10-24：用户明确授权测试及 CI 镜像构建。

修复：允许选择有写权限的 User/Web 运行网络，在保存事务内纳管 User 配置并添加服务自有映射，保留权限和中央托管限制。

新增 11 个 Rust 回归用例，覆盖来源组合、只读/中央托管拒绝、实例与状态校验、证明范围、纳管与取消公开保留手工规则、失活账号、配置代次和端口冲突回滚。

本地未执行 Rust 测试（无 Cargo 工具链）。实际测试由修复提交的原 amd64 GHCR 流程执行；原生阶段的 `publication-tests.log` 随 `ghcr-native-report-amd64` 上传。测试失败阻止发布，本记录不预先声称 CI 成功。

详细记录：`process/service-publication-user-network-fix-in-progress/task/test-report.md`。
