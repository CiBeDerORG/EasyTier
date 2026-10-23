# P01 账号状态与禁用网络

依据当前`db/entity/users.rs`、`restful/users.rs`、`central_network/device_auth.rs`、`client_manager/session*`、`db/mod.rs`及`central_network/service.rs`实现。users新增nullable deleted_at；原账号默认正常，保留用户名唯一约束。管理员授权按当前用户ID查admins成员关系，不能使用原来漏actor过滤的权限查询。

新`db/user_admin.rs`提供管理员列表/删除事务与is_user_active；新`restful/user_admin.rs`挂接JSON接口。删除在现有CentralNetworkService业务写锁下提交，网络停止结果单独反馈，不能把提交后的RPC超时描述成账号删除回滚。

网络owner处理统一配置写入口、状态启用、中央意图发布与运行同步屏障；认证owner处理密码/OIDC/旧会话及config-server本地校验，提供仅暂停后台任务但保留RPC通道的方法供网络owner调用。真实停止需要RPC，离线只承诺持久禁用。

集成时补齐`restful/network.rs`及`restful/rpc.rs`：网络修改在取得同一业务锁后复核账号状态；internal路由同步注入CentralNetworkService，阻止在删除之后排队执行的旧请求先启动运行实例再因数据库写入被拒绝。现有internal路由fixture随依赖注入更新，未执行该用例。

升级增加`m20261023_000014_user_soft_delete`；已有账号默认deleted_at为空。出现删除标志后禁止直接撤销迁移，以免账号复活。后续回退须用匹配的升级前备份和旧制品，不能单换旧二进制读取本轮已删除账号的数据库。

任务T01/T02/T03可按文件独占并行；T05集成前核对签名、外键保留、所有source和过期会话竞态。测试范围待集中授权，不用旧339项结果代表新版本通过。
