# 自动化中心任务看板

基线f951e054，工作分支feat/automation-center。统一契约：[R10](../require/require-10-implementation-contract.md)。每个目录只有一位实现负责人，共享入口由root集成。当前未执行任何测试。

|编号|任务|负责人|进度|依赖|
|---|---|---|---|---|
|T01|[数据与目标解析](task-01-storage.md)|backend_storage|源码完成，待验证|R10|
|T02|[接口与授权代理](task-02-api.md)|backend_api|源码完成，待验证|T01 Store签名、T03协议|
|T03|[手机执行器与脚本](task-03-executor.md)|executor_runtime|源码完成，待验证|R10、T04接口|
|T04|[WebRTC媒体与控制](task-04-media.md)|executor_media|源码完成，待验证|R10媒体接口|
|T05|[手机工作台](task-05-workbench.md)|frontend_workbench|源码完成，待验证|R10、T06面板|
|T06|[脚本与执行记录页面](task-06-scripts-ui.md)|frontend_scripts|源码完成，待验证|T05 API/types|
|T07|[集成与源码复审](task-07-integration.md)|root|源码接线完成，待验证|T01–T06|

接口或字段变更需直接通知相邻负责人并更新R10；开发完成不等于测试通过。测试范围及未执行原因记录在[test-report.md](test-report.md)。

本轮已记录的阻塞：调试模式尚未授权执行验证，真实Android/ICE/TURN和迁移均未运行。更换执行器的观看会话身份边界见[需求汇总](../README.md)；不要在线切换正在工作的执行器。
