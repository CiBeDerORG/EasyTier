# R04 常见方案调研与选型

> 阶段更新（2026-10-24）：产品稿之后，用户已要求说明后端服务架构和开源/自研边界。当前建议统一为scrcpy＋aiortc/WebRTC＋uiautomator2，按需coturn，详见R02；早期WSS/WebCodecs首选已调整。以下来源与候选比较保留；所有选型仍是未实施、未实测的建议。

调研日期：2026-10-24（Asia/Shanghai）。方式：官方GitHub源码、README、LICENSE及Git元数据的只读检查。未安装、运行、压测或连接真实手机。下述版本是当次取得的HEAD或源码字段，不代表经验证的稳定发行版。

## 1. 结论

本项目当前首选组合：**scrcpy采集/人工控制 + aiortc/WebRTC媒体 + openatx/uiautomator2控件执行 + EasyTier自己的资产授权、步骤编辑和任务记录**，公网需要时使用开源coturn中继。

原因是用户要在已有Web控制台集中管理较小规模远程手机，既要画面和群控，也要可重复的操作脚本。没有一个候选同时提供这些能力且能无成本接入当前账号/资产模型。分层选取成熟部件，比完整导入另一套设备平台更吻合现有项目。

重新核对EasyADB发现，它已存在手机预编码H.264经aiortc转为WebRTC发送的应用路径，可作为Python执行器的媒体适配基础。无需为了协议栈先增加另一种语言的媒体服务；浏览器可达性、TURN、关键帧恢复、H.264兼容和并发容量仍需验证。WSS/WebCodecs保留为特定网络或浏览器条件下的备选，不同时建设两套首发媒体链路。EasyADB提供可复用代码，不整体复制其设备库、全局Token及会话抢占行为。

## 2. 候选对比

|方案|擅长|主要代价/缺口|本项目选择|
|---|---|---|---|
|scrcpy|硬件编码实时画面、触控/键盘、USB/TCP ADB|官方是桌面工具；Web、群控、脚本、账号授权需适配|选为采集和人工控制底座|
|Tango / ya-webadb|TypeScript ADB/scrcpy协议、WebCodecs和软件解码组件|库集合，不是多租户设备中心；WebUSB连接浏览器本机USB，不能直接接管远端手机池|作为WSS/WebCodecs备选及协议参考，当前WebRTC首版不额外引入一套播放器|
|DeviceFarmer/STF|完整Web设备农场、占用/预约、多用户、APK/远程ADB管理|引入另一套用户资产；RethinkDB/ZeroMQ等多个服务，媒体有minicap JPEG路径|首版不整体引入，适合独立企业测试设备池|
|openatx/uiautomator2|Python控件查询、点击/输入、等待/断言、截图、应用操作|仍需自己的步骤编辑/任务服务；需ADB授权和手机端运行组件|选为原生Android脚本主适配器|
|Appium + UiAutomator2 Driver|WebDriver标准、多语言测试生态、原生/WebView支持|Node/JDK/Android SDK/server/driver及并行端口配置，额外服务较多|有复杂WebView或标准测试体系需求时再接|
|Maestro|YAML Flow、自动等待、条件/复用、多设备分片执行|嵌入式交互工作台仍需自建；Studio Desktop非开源，CLI需要Java和手机驱动|脚本流程优先的有力备选，后续可独立Runner适配|
|Airtest|图片定位、等待/断言、报告、Python自动化|依赖画面样本与阈值；图片匹配不等于OCR；IDE另行分发|后续用于自绘/游戏画面补充|
|Poco|原生控件树与接入SDK后的游戏控件|游戏通常需集成SDK；Android instrumentation与其他引擎有冲突|不作为默认原生App执行器|
|AutoJs6|手机本地无障碍脚本、录制、图像/OCR、离线自主执行|逐机App/权限/后台保活与升级；中心调度仍需开发|仅有手机脱离ADB自主运行需求时再接|
|纯ADB input|简单坐标、按键、基础Shell动作|缺乏稳定控件和结果判断，文本输入有限|基础动作兜底，不作为完整脚本引擎|

uiautomator2与Appium的UiAutomator2 Driver不是同一软件包。两者都利用Android UIAutomator能力；选前者是因为适合Python执行器内有限动作封装，并非认定Appium能力不足。

Maestro提供现成流程执行器，若用户最主要需求是直接编写YAML测试而非网页边看边操作、录制编辑和逐步调试，Maestro可以上升为主选。目前场景更强调交互工作台，因此首选直接的控件API。不要同时为首版建设两套脚本运行器。

## 3. 维护与许可证观察

|官方仓库|所读HEAD|提交日期/源码说明|仓库许可|
|---|---|---|---|
|Genymobile/scrcpy|a60891aea193d92e7e5c3942700eca63f9d19a5f|2026-10-08；README v5.0.1|Apache-2.0|
|DeviceFarmer/stf|e236d28fcd0a5bdac8a254875fc7a962a7ac95ea|2026-10-23|Apache-2.0|
|yume-chan/ya-webadb|7bcec613f3441a7f20bdf3e5e8646bce63fe790d|2026-10-09；release说明v3.0.0-beta.4|MIT|
|openatx/uiautomator2|3adba4a59382bf64cf0eee515f9d9102b5acff99|2026-10-23；手机u2.jar更新|MIT|
|appium/appium-uiautomator2-driver|8d0128ac9d5782aa3e9f0cf1c773805280970a91|2026-10-19；package 8.7.0|Apache-2.0|
|appium/appium|a6a69d36d34079c85bde89f0725e230fbc3ff999|2026-10-23；package 3.8.0|Apache-2.0|
|mobile-dev-inc/maestro|436309bdbd0aca29450a3f0c7fcf8bbe9c3ab027|2026-10-23|CLI Apache-2.0；Studio非开源|
|AirtestProject/Airtest|d729c631d2032521be1e2168a249a2a89d79af2a|2025-12-04；源码1.4.3|Apache-2.0，不自动覆盖另行下载的IDE|
|AirtestProject/Poco|2481259f64c9f9fd4c78a611bd57880558144113|2024-01-08|Apache-2.0|
|SuperMonster003/AutoJs6|ed3eb10e88db5a8425fd94bdddefa4176e5e1c94|2026-03-16；README记录v6.7.0|MPL-2.0|

最近提交不等于可靠性保证，较旧默认分支也不能独自证明项目停止维护。STF有2026-10提交，不能沿用“STF已停止维护”的泛泛结论。其当前README Node要求与package engines存在差异，若选它需按固定版本验证实际运行依赖。

Tango所读HEAD带beta版本说明；实际组件版本、scrcpy服务端与控制编码器应锁定兼容组合后验证。uiautomator2采用动态版本，不能把pyproject默认值当作最新正式版本。AutoX旧URL`kkevsekk1/AutoX.git`本次返回Repository not found，仅记录该地址不可取，不推断所有分支状态。

## 4. 容易误判的能力

- WebUSB只能选择浏览器所在机器可访问的USB设备。远程OpenWrt/NAS手机仍需服务器ADB；Tango可只复用解码库。
- scrcpy内部协议会随版本变化，设备端server和客户端适配器必须兼容。不能每次下载最新server配旧编码器。
- Appium截图/MJPEG、Maestro录屏产物不能自动等同多路低延迟远控。实时媒体和脚本分开设计。
- Maestro当前已有Unicode输入，不能引用旧资料说不支持中文；uiautomator2也有中文示例，但可能安装辅助IME。Airtest Unicode输入同样可能安装/切换IME。必须把手机端准备讲清楚。
- Maestro的`record`涉及运行视频，不等于可直接嵌入的操作录制编辑器；其Studio Desktop不是开源组件。Airtest框架开源也不自动意味着AirtestIDE可嵌入。
- Poco的Android驱动明确会与其他instrumentation冲突，甚至尝试停止/卸载旧组件。每台只选择一种控件驱动，不把Appium、Poco、uiautomator2混跑。
- uiautomator2 v3已移除旧atx-agent常驻服务；首版不启用watch_context。Maestro活动session有每5秒本地心跳；现有EasyADB投屏也有活动会话心跳。这些不等于业务cron，但必须如实说明。

## 5. 官方来源

### 媒体与设备农场

- [scrcpy README](https://github.com/Genymobile/scrcpy/blob/a60891aea193d92e7e5c3942700eca63f9d19a5f/README.md)、[协议](https://github.com/Genymobile/scrcpy/blob/a60891aea193d92e7e5c3942700eca63f9d19a5f/doc/develop.md)、[输入方式](https://github.com/Genymobile/scrcpy/blob/a60891aea193d92e7e5c3942700eca63f9d19a5f/doc/keyboard.md)。
- [Tango README](https://github.com/yume-chan/ya-webadb/blob/7bcec613f3441a7f20bdf3e5e8646bce63fe790d/README.md)、[WebCodecs组件](https://github.com/yume-chan/ya-webadb/blob/7bcec613f3441a7f20bdf3e5e8646bce63fe790d/libraries/scrcpy-decoder-webcodecs/README.md)、[WebUSB](https://github.com/yume-chan/ya-webadb/blob/7bcec613f3441a7f20bdf3e5e8646bce63fe790d/libraries/adb-daemon-webusb/src/manager.ts)。
- [STF README](https://github.com/DeviceFarmer/stf/blob/e236d28fcd0a5bdac8a254875fc7a962a7ac95ea/README.md)、[Compose](https://github.com/DeviceFarmer/stf/blob/e236d28fcd0a5bdac8a254875fc7a962a7ac95ea/docker-compose.yaml)、[浏览器画面](https://github.com/DeviceFarmer/stf/blob/e236d28fcd0a5bdac8a254875fc7a962a7ac95ea/res/app/src/features/control/screen/screen-stream.ts)。

### 控件与流程自动化

- [uiautomator2架构](https://github.com/openatx/uiautomator2/blob/3adba4a59382bf64cf0eee515f9d9102b5acff99/README.md)、[v3迁移](https://github.com/openatx/uiautomator2/blob/3adba4a59382bf64cf0eee515f9d9102b5acff99/docs/2to3.md)、[选择器/中文输入](https://github.com/openatx/uiautomator2/blob/3adba4a59382bf64cf0eee515f9d9102b5acff99/README_CN.md)。
- [Appium Driver要求](https://github.com/appium/appium-uiautomator2-driver/blob/8d0128ac9d5782aa3e9f0cf1c773805280970a91/README.md)、[并发指南](https://github.com/appium/appium-uiautomator2-driver/blob/8d0128ac9d5782aa3e9f0cf1c773805280970a91/docs/guides/parallel-tests.md)、[Unicode输入](https://github.com/appium/appium-uiautomator2-driver/blob/8d0128ac9d5782aa3e9f0cf1c773805280970a91/docs/reference/execute-methods.md)。
- [Maestro README](https://github.com/mobile-dev-inc/maestro/blob/436309bdbd0aca29450a3f0c7fcf8bbe9c3ab027/README.md)、[Android Unicode实现](https://github.com/mobile-dev-inc/maestro/blob/436309bdbd0aca29450a3f0c7fcf8bbe9c3ab027/maestro-client/src/main/java/maestro/drivers/AndroidDriver.kt)、[多设备参数](https://github.com/mobile-dev-inc/maestro/blob/436309bdbd0aca29450a3f0c7fcf8bbe9c3ab027/maestro-cli/src/main/java/maestro/cli/command/TestCommand.kt)。

### 图像与手机本地脚本

- [Airtest README](https://github.com/AirtestProject/Airtest/blob/d729c631d2032521be1e2168a249a2a89d79af2a/README.md)、[Unicode输入与IME](https://github.com/AirtestProject/Airtest/blob/d729c631d2032521be1e2168a249a2a89d79af2a/airtest/core/android/ime.py)。
- [Poco能力](https://github.com/AirtestProject/Poco/blob/2481259f64c9f9fd4c78a611bd57880558144113/README.rst)、[instrumentation冲突](https://github.com/AirtestProject/Poco/blob/2481259f64c9f9fd4c78a611bd57880558144113/poco/drivers/android/uiautomation.py#L300)。
- [AutoJs6 README](https://github.com/SuperMonster003/AutoJs6/blob/ed3eb10e88db5a8425fd94bdddefa4176e5e1c94/README.md)、[MPL-2.0](https://github.com/SuperMonster003/AutoJs6/blob/ed3eb10e88db5a8425fd94bdddefa4176e5e1c94/LICENSE)。
