# T02 网络禁用闭环

network-owner独占db/mod.rs、db/central_intent.rs、central_network/service.rs、client_manager/mod.rs及必要runtime_revision。阻止已删账号中央重发布、普通/运行来源配置重新启用；不删除网络意图/成员/配置。提供账号级有界停网方法，先暂停已有后台worker、停止在线实例、断管理会话，移除该owner Gatewayruntime。离线及失败必须保留未确认反馈，不新建任何后台任务。

root调用期间已持有service mutation_lock，不在内部再次获取同一锁。若数据库或runtime不能读取，返回明确失败信息，不能以0计数伪装全停。交接给root/API及前端的报告字段须一致。
