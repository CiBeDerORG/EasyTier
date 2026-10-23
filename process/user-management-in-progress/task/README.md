# 任务看板

|任务|范围/唯一owner|依赖|状态|
|---|---|---|---|
|[T01](task-01-account-state-and-auth.md)|迁移、用户实体、登录/OIDC、本地客户端认证；auth-owner|方案|已实现，未验证|
|[T02](task-02-network-disable.md)|db/mod/central_intent、中央service、client manager及运行同步；network-owner|T01状态契约|已实现，未验证|
|[T03](task-03-admin-store-and-api.md)|新db/user_admin、restful/user_admin、REST注册；root|T01/T02接口|已实现，未验证|
|[T04](task-04-user-management-page.md)|菜单、路由、ApiClient、用户页、cn/en；frontend-owner|冻结API|已实现，未验证|
|[T05](task-05-integration-and-review.md)|文档、DTO接线、REST/runtime写入竞态人工审阅；root|T01–T04|人工审阅完成，待授权执行验证|

多agent文件边界：auth不写db/mod.rs（network独占）；root的新Db方法位于user_admin模块，模块注册由network合入；Session暂停后台方法由auth提供，network仅调用。root不与frontend争写API/翻译文件。需修他人范围先交owner，避免覆盖。
