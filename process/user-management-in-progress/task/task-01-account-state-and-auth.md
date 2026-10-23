# T01 账号状态与认证

auth-owner独占000014迁移及注册、users实体、restful/users与oidc、DeviceAuth、session与webhook_validation。新增deleted_at，拒绝已删除账号登录/旧会话/OIDC自动复活/config-server本地授权；保持原协议，外部认证原机制不误改。补已有权限查询的actor过滤，核对新增实体字段构造处。Session提供暂停后台worker而保留RPC的交接方法。

完成标准：root接到精确签名、错误映射、受影响构造位置和未测清单；本轮不执行测试/构建。
