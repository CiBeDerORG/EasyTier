# T02 接口与授权代理

负责人backend_api。唯一写入restful/automation目录。提供R10 HTTP、WebSocket、内部授权及结果接口；普通请求检查活跃账号/归属，内部请求独立Bearer认证。执行器地址只取管理员环境配置，禁任意URL与跳转。运行先持久化再派发；会话续授权只发生于用户打开的连接内。

交接T01 Store、T03 Runner及T05前端协议。root只合并router和Cargo特性。待测会话越权、Origin、关闭、目标变更、迟到/取消和不可达结果；未执行。
