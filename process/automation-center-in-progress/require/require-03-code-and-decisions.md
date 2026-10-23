# R03 项目代码依据与实施前决策

## 1. 实际基线

2026-10-24读取远端`feat/node-attributes-tech-console`，HEAD为`bc792224`。独立工作区`/workspace/EasyTier-tech-console-review`仅用于本轮阅读和方案文件；原工作区及其未提交ADB代码保留，未切换覆盖。

|项目事实|源码依据|对方案的影响|
|---|---|---|
|现有科技风控制台可扩展工作区导航|`easytier-web/frontend/src/components/MainPage.vue:204`|增加一级自动化中心；沿用主题和账号外壳|
|路由在main.ts定义，使用hash history|`easytier-web/frontend/src/main.ts:46`、`:112`|新页面放在`/h`下，不另部署一套前端登录|
|实体设备有owner/node、网关、IPv4、实例及绑定版本|`easytier-web/frontend/src/types/node-assets.ts:267`|用于自动化目标与地址冻结|
|地图另有MapLanDevice模型|`easytier-web/frontend/src/modules/network-space.ts:12`|不能凭地图记录ID/IP直接下发控制；不新建第三套资产|
|设备类型可自定义改名|`easytier-web/src/db/assets/types.rs:58`|手机类型不作为能力授权依据|
|当前资产设备查询按一个节点限定|`easytier-web/src/db/assets/devices.rs:17`|增加服务端跨节点聚合接口，避免前端逐节点请求拼手机墙|
|旧资产授权只检查owner存在|`easytier-web/src/restful/assets/mod.rs:184`|新控制必须补有效账号校验和运行中撤销|
|用户逻辑删除保留资产并禁用网络|`easytier-web/src/restful/user_admin.rs:119`|追加自动化会话和任务取消，不因保留资产继续允许控制|
|配置用全局mutation mutex|`easytier-web/src/central_network/service.rs:241`|不能在整个投屏/脚本期间持有|
|asset_operations处理CRUD幂等|`easytier-web/src/db/assets/idempotency.rs:35`|新建任务执行记录，不把该表当自动化运行引擎|
|服务公开已有独立处理|`easytier-web/src/restful/assets/publication_apply.rs:151`|公网映射不等于视频可达或ADB已授权|

## 2. 上一轮代码与新分支的差异

本分支尚无上一轮工作区新增的`AssetAdbDialog.vue`、Discovery/ADB REST和DB模块。因此“已写入旧工作区”不能当作“最新远端分支已具备”。

当前迁移注册已有`m20261023_000015_service_publications`、`m20261023_000016_node_geo_locations`及network_space；旧工作区新增的是20261024日期下000015/000016 Discovery/ADB迁移。完整名称不同，不能称为确定的同名冲突，但适配时必须重新核对依赖、顺序、前后端DTO和新服务公开行为，不能原样假定已合并。

新方案先复用资产、ADB授权/连接和精确信息读取的合理部分，再决定独立执行器适配；不强制保留旧EasyADB接口作为唯一实现。

## 3. EasyADB可复用事实

本地参考基线`dad5f406`，其上一轮配套集成接口仍是未提交改动。

- `docs/M2-WEBRTC.md:18`：20个逻辑会话、默认3路并发启动；上限不代表实机容量承诺。
- `server/m2_quality.py:10`：省流档960/3Mbps/24fps；自定义最低仍为960/2Mbps/15fps，更低负载手机墙档位需要新增。
- `web/m2-group.js:147`：批量文字和导航键；`web/m2-multi.js:1341`触摸只发单卡片，主控触摸广播尚无实现。
- `server/m2_keys.py:18`：当前只接受可打印ASCII；上游scrcpy其他输入能力不能直接算成本地EasyADB已实现。
- `server/services/session_manager.py:72`：单设备会话接管会影响旧客户端，不能直接当作多观看者共享方案。
- `server/api/live_handler.py:575`：输入已接受只表示写入控制socket，不证明App业务成功。
- `docs/M2-WEBRTC.md:54`：当前媒体依赖可路由UDP，未实现STUN/TURN。
- `server/m2_webrtc.py:227`、`:233`及`server/m2_live.py:399`：已存在预编码H.264封装为`av.Packet`并交给aiortc轨道的路径，不是应用层解码为VideoFrame再编码。可复用适配，不能据此宣称新分支已接入或并发达标。
- `server/api/live_handler.py:387`：`iceServers=[]`；公网适配需配置浏览器与执行器两端的ICE、短期TURN凭据及可达端口。现有应用代码未见PLI到手机关键帧请求、网络反馈到手机码率调整的闭环，不能仅因采用WebRTC就承诺已有自动自适应。
- 新增的`server/services/easytier_integration.py`只含配对/连接/读取，并主动拒绝活跃投屏目标；脚本不能逐步调用这个一次性接口来代替会话控制。

## 4. 建议先按以下假设推进方案

1. 浏览器不要求加入EasyTier；媒体首选WebRTC，复用已有aiortc预编码包路径，需要时经coturn中继。网页HTTPS可达不等于媒体一定可达，S0需验证真实浏览器网络、直连及强制中继。此前WSS/WebCodecs首选假设已被当前R02服务架构建议取代。
2. Android原生App是首要对象，控件脚本优先uiautomator2；大量游戏/Canvas需要增加图像定位适配。iOS、虚拟屏实验、多指手势不列入首版。
3. 首轮10台同时观看，设备逐级并发执行；未承诺20台实测指标。手机总数可以更多，实时开启数量遵守容量预算。

三项实施前关键待确认：实际常用App/操作流程；同时观看和同时执行数量；中文/特定机型输入与网页访问网络。方案无需等待这些答案才能形成，但它们会改变验收设备与阶段范围。

当前服务拆分建议为现有Rust/Axum业务主服务与独立Python/FastAPI手机执行器，按需增加coturn；架构建议详见R02。没有修改现有登录、config-server认证、网络实例或设备页面业务；本轮仅文档。

## 5. 架构深化复审（2026-10-24）

- R08补部署与运行时：现有EasyADB WebRTC compose要求host网络且uvicorn只有一个worker；会话锁/配额在进程内。单队列H.264 track不能直接当多观看者广播。当前建议同手机首版一位观看者、独立Peer，观看/控制/采集/任务分别释放。
- 地址复用现有direct_instance/gateway_rule/gateway_self派生和config_ref规则；新增target_fingerprint不等于已有preview_version。无改址且网络间地址冲突时，单Runner普通路由不能凭资产ID区分手机。
- R09补可靠交接：SQLite事务先保存运行与dispatch_pending；Runner持久接收/结果；取消与实际停止分开；旧请求可能迟到，必须有持久拒收记录，不能以404为安全重试证据。控制代际与撤销状态不能仅在内存。
- 账号软删、资产绑定变化和删除需要新增事务内失效登记；不能持现有网络mutation_lock跨远程操作。资产删除保留历史、未确认取消和清理记录。
- 产品草案收敛：任务暂停及任意中途续跑后置，保留录制暂停、任务取消与明确新运行。先证明手写有限步骤引擎，再接录制/同步手势，首版总目标未删减。
- 三路agent只读复审并对新增文档交叉审稿；没有执行测试、基准或真实设备操作。开源候选沿用R04已记录来源，本轮没有重新联网调研。
