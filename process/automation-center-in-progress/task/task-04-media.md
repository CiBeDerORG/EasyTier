# T04 WebRTC媒体与实时控制

负责人executor_media。唯一写入automation_executor/media.py与vendor目录。参考EasyADB最小H.264/scrcpy代码并保留来源，提供R10 async MediaManager接口；不带入独立资产库、全局浏览器token或解锁密码逻辑。处理ICE、单观看者、几何版本、背压和真实清理结果。

给T03交付create/offer/command/close/close_all。待测真实手机H.264、旋转、黑边、TURN、丢包恢复和单台故障隔离；未执行。
