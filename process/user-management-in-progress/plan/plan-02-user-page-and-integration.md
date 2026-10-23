# P02 用户管理页面与API

基于现有MainPage工作区菜单、PrimeVue布局和ApiClient，新增/h/users，不改原节点/实例页面。权限上下文首次或进入页时按需获取，不加入已有10秒导航timer。API每次独立授权；隐藏菜单不代替后端检查。

- GET `/api/v1/admin/users/context`：actor、available、can_manage_users。
- GET `/api/v1/admin/users`：items含id、username、is_admin、is_current_user、deleted_at、node_count、network_instance_count、disabled_network_instance_count、can_delete。
- DELETE `/api/v1/admin/users/{id}`：无body，返回user_id、deleted_at、already_deleted、disabled_network_instance_count、runtime_error、runtime（attempted_nodes、confirmed_nodes、pending_nodes、removed_gateway_instances）。运行结果整体无法取得时runtime=null、runtime_error=true；账号删除已提交，仍返回200，由页面提示运行状态未确认。
- 错误返回code/message，401/403清理敏感列表，临时失败保留已加载数据并允许手动重试。

已删除用户保留一行状态，不能恢复，可重试禁用网络；删除正常用户需确认资料保留和网络停用影响。复制旧模板时不得新增自动请求或打印用户敏感信息。T04独占前端，T05最后交叉接线。

前端读取超时15秒，删除超时60秒，仅作用于新增接口。超时显示结果未知，允许重复同一DELETE；局部更新已确认的删除结果后再刷新列表。菜单在首次加载、页面进入或必要的用户导航时查询权限，不接入已有10秒定时器。
