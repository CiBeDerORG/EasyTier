# T03 手机执行器与脚本

负责人executor_runtime。唯一写入automation-executor目录，但media.py及vendor由T04独占。实现单worker FastAPI、ADB明确配对/连接/读取/脚本准备、journal和重启处理、有限uiautomator2步骤、取消与有界资源回收、主服务授权/回调、独立镜像与部署说明。

不执行全网扫描、自动重连、任意脚本或命令；不把API接收当手机成功。待测手机授权/IME、任务关闭网页后继续、重启不重放、拒收迟到命令、资源清理；未执行。
