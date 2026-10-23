# T03 管理员列表和删除事务

root独占db/user_admin.rs、restful/user_admin.rs及restful/mod.rs接线。统一is_user_active供其他模块使用。列表无敏感字段，DELETE在事务内检查当前有效管理员、自删与最后管理员；重复请求保留首次删除时间。禁用全部持久实例、清revision与账号状态原子提交，节点/设备/服务及网络意图不删。持久删除成功与RPC结果分别表达。

本轮只写源码并人工审阅，不运行迁移/服务/测试。

状态：已实现，未执行验证。集成追加root负责的restful/network.rs、restful/rpc.rs：在原有或新增的共用业务锁内检查账号；internal路由提供同一服务Extension，现有独立fixture同步注入。
