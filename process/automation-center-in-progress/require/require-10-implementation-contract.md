# R10 首版实施契约

基线f951e054；开发分支feat/automation-center。以下是多个agent共用的接口；变动先通知对接方和root，不在各层独立发明字段。root负责共享注册文件。调试模式不执行任何测试/构建/lint/服务。

## 服务与职责

- 现有Rust主服务负责登录、活跃actor/owner、资产解析、SQLite脚本/会话/任务及撤销。
- 新目录automation-executor/提供Python/FastAPI服务，ADB/scrcpy/aiortc/uiautomator2、单进程注册表、有限脚本子进程、本地SQLite journal。
- 首版单个配置执行器，无新执行器CRUD页面；由环境变量配置。不提供无人值守重连、扫描或cron。
- Rust配置EASYTIER_AUTOMATION_RUNNER_URL和EASYTIER_AUTOMATION_RUNNER_TOKEN。执行器配置AUTOMATION_TOKEN及AUTOMATION_CALLBACK_URL（主服务私网地址，内部API共用token），密钥只在服务端。
- 浏览器全部业务/信令/控制经现有登录态Rust入口，不直接持执行器密钥。Rust将WebSocket小控制消息代理到执行器，视频通过WebRTC直连/TURN，不进Rust/数据库。这是首版对短期浏览器票据方案的简化：认证代理取代直接暴露Runner。
- 每次手机会话初始有效期60秒，Rust仅在该已打开WebSocket存续期间每20秒检查授权/目标并续执行器租期；输入也检查。任务每步骤向主服务确认授权，单步骤有最长时限。仅为用户已启动会话的资源治理，不扫描设备或新增业务定时器。Runner到期不能继续控制/观看；浏览器断开关闭人工会话，脚本独立运行。

## 浏览器HTTP API

前缀/api/v1/automation，所有管理API使用owner_user_id查询参数；管理员沿用既有assets/accounts与context，不允许外部webhook模式。

|方法/路径|请求|响应|
|---|---|---|
|GET /context|无|{available,runner_configured,actor:{id,username},can_manage_others,protocol_version:1}|
|GET /devices|owner_user_id|{items:AutomationDevice[]}，全账号跨节点；前端显式查询/本地筛选|
|PUT /devices/{id}/connection|owner；{enabled,adb_port,connection_mode:"tcp"或"wireless"}|更新后的AutomationDevice|
|POST /devices/{id}/prepare|owner；{action:"connect"或"pair"或"inspect"或"automation",pairing_port?,code?}|{state,message,metadata?,capabilities?}；配对不存码，显式连接；automation明确准备uiautomator2/中文输入能力，inspect不安装|
|POST /sessions|owner；{device_id,quality:"wall"或"focus"}|{id,device_id,state,ice_servers,expires_at}|
|POST /sessions/{id}/offer|owner；{sdp,type:"offer"}|{sdp,type:"answer",width,height}|
|GET /sessions/{id}/control|owner；WebSocket升级|下述控制协议；授权和目标变化时关闭|
|POST /sessions/{id}/close|owner；{}|{id,state,cleanup_pending?}|
|GET /scripts|owner|{items:AutomationScript[]}|
|POST /scripts|owner；{name,description,steps}|AutomationScript|
|PUT /scripts/{id}|owner；{name,description,steps,revision}|AutomationScript；revision冲突409|
|DELETE /scripts/{id}|owner|{deleted:true}；不删历史快照|
|POST /runs|owner；{request_id,script_id?,revision?,name?,steps?,device_ids,parameters:{},concurrency:1..3}|AutomationRun；script_id或草稿steps二选一|
|GET /runs|owner|{items:AutomationRun[]}|
|GET /runs/{id}|owner|AutomationRun（本地已保存结果）|
|POST /runs/{id}/refresh|owner；{}|向Runner一次查询再落库返回AutomationRun|
|POST /runs/{id}/cancel|owner；{}|AutomationRun，远端不明显示cancel_requested|
|GET /runs/{id}/artifacts/{artifact_id}|owner|鉴权后代理该任务已登记的PNG/JPEG截图，单张最多8MiB，不缓存|

首版未提供SSE；执行器回调更新数据库，页面通过明确的“刷新”操作读取结果，不做轮询。

AutomationDevice字段：id,name,model,owner_user_id,machine_id,node_name,gateway_group_id,gateway_name,device_type_id,type_name,overlay_ip,lan_ip,binding_state,connection:{enabled,adb_port,connection_mode},target_fingerprint,availability_reason,active_session。active_session为空或{id,state,actor_user_id,expires_at}，允许刷新页面后明确清理之前的观看会话。默认列出现有资产并通过显式配置启用；手机类型不作为权限依据。

AutomationScript：id,name,description,revision,steps,created_at,updated_at。
AutomationRun：id,name,state,created_at,updated_at,script_snapshot,executor_id,parameter_names,cancel_requested,devices:[{device_id,name,state,current_step,error?,cleanup_confirmed?,steps:[{step_id,state,message?,started_at?,finished_at?,artifact_id?,mime_type?,size?}]}]。运行参数值仅在本次派发与执行内存中使用，数据库只保存参数名与请求摘要。

错误统一{code,message,field_errors?}及恰当HTTP状态；不回显配对码/输入内容。缺少执行器配置时返回可操作说明，界面不展示伪造在线设备。

## 控制WebSocket

客户端连接后服务端发送{type:"ready",session_id,device_id}。
客户端每条消息带command_id（UUID）、seq递增，type可为：
- touch：action down/move/up，x/y为视频像素，width/height为当前video尺寸。touch.release用于取消手势。
- key：key HOME/BACK/RECENT/WAKE。
- input_text：text，允许中文由有限控件适配处理，最大2000字节/合理长度。
- stop：释放本次人工输入；关闭用close接口/WS关闭。
响应{type:"ack",command_id,state:"action_sent"}或{type:"error",command_id,code,message}；命令回执不等于App成功。
媒体控制当前仅一个观看者；脚本运行占用控制权，观看允许。输入通道一次最多一个请求在途，MOVE可合并但DOWN/UP必须保持顺序；悬浮控件不穿透手机。全局按钮冻结已选会话，单机按钮只发单设备。

前端先接通控制WebSocket并收到ready，再发起ICE/offer，协商等待期间租期可正常续验。显式“释放手控”发送stop并等待结果，保留观看；停止同步同样释放人工占用。切换脚本面板前释放手控，脚本不能抢走已有写入控制权。页面真正离开或隐藏时关闭人工会话，已启动脚本按固定任务独立执行。

## 有限步骤schema v1

步骤对象有id（稳定UUID）、type，以及以下动作字段，后端和执行器均校验，禁止任意shell/Python。
- key：key HOME/BACK/RECENT/WAKE。
- tap：selector?或x,y（0..1），坐标需width,height与画面方向匹配。
- swipe：x,y,to_x,to_y（0..1），duration_ms 100..3000，width,height。
- input_text：text或parameter（运行参数key），selector?；不允许两者同时。
- launch_app：package（Android包名）。
- wait_for / assert：selector，timeout_ms 100..30000。
- screenshot：无额外字段，按需保存检查点。
selector只能是{resource_id?,description?,text?}非空且字符串受限；多个匹配失败，找不到可在本步有限等待。
最多100步；首版仅有限顺序执行。每设备参数可后续扩展，当前parameters为本次公共字典。录制生成坐标tap/swipe/key/input_text草稿；敏感输入不自动持久化。

## Rust Store与生命周期

T01拥有db/automation/**和新迁移m20261024_000018_automation_center.rs（注册root负责）；T02拥有restful/automation/**。两者直接协商Store函数签名，使用现有AssetError或独立兼容错误，不在共享文件抢写。
- 复用AssetsStore绑定状态计算，拒绝changed/replaced/missing/unavailable；目标基于固定overlay_ip，禁止地址回退；明确route_context为当前单Runner默认域。
- 复用既有资产RuntimeObservations，按(owner,node)分别保存实例观测。手动查询、创建会话、活动会话续验及脚本步骤授权只查询相关节点，兼容配置IP、中央分配IP及运行态DHCP IP。控制WS在两次续验之间复用已核验观测，每条输入仍检查数据库权限和绑定，避免每次触摸都发节点RPC。无法取得必要观测时拒绝继续，不用旧IP猜测目标。
- 一个执行器/ADB身份只有一个路由域，跨账号相同实际IP和ADB端口不能复用；禁用连接不等于释放另一账号的手机授权。未闭合会话及任务快照也参与冲突检查。这不能证明物理设备所有权，不支持重叠地址自动选路。
- 数据库短事务写任务及明细后才派发；请求幂等键+摘要防重复。
- 执行器journal保存稳定executor_id，重启只改变generation；主服务先记录执行器身份再发送任务，执行器仅接收expected_executor_id匹配的派发/取消。未绑定身份的任务取消会封死后续派发；已绑定任务只有同一执行器持久拒收且证明never_started时，才能据此确认从未开始。journal丢失或更换执行器身份不作此证明。
- 任务结果使用单调递增revision，事件ID防重复，固定设备和步骤快照防串任务。乱序旧快照不能让已完成设备回到运行状态。
- “动作结果未知”和“控制进程已清理”分开记录。显式取消后，同身份执行器确认该任务无后续执行、子进程及控制占用已释放，才能写入cleanup_confirmed并解除互斥；旧outcome_unknown结果继续保留。用户先核对手机实际情况，再主动创建新任务，不自动重跑未知动作。
- 会话/任务历史存快照，资产删除不级联抹记录；取消/撤销保留。
- 持久撤销：迁移触发器或现有删除事务钩子在users.deleted_at改变、asset_devices删除/相关路由字段更新时使会话revoked、运行cancel_requested。T01实现数据侧钩子；root处理共享源码的必要即时通知。
- 活跃账号/权限/绑定检查用于会话续期、控制及每脚本步骤。变更在当前步骤有界超时后不再执行，Runner不可达状态真实保留。
- 部署只支持单Runner，活动观看期间不支持更换其URL或数据卷。会话记录尚无持久执行器身份；更换须先关闭会话并停止旧Runner，异常旧观看靠剩余最多60秒租期终止，不能把新Runner的close回复解释为旧机器已即时清理。此限制不适用于已严格校验executor_id的任务记录。
- 账号/资产原有CRUD、OIDC、config-server认证和部署Agent流程不变。

## Runner HTTP v1

全部/internal/v1路径要求Authorization: Bearer AUTOMATION_TOKEN，不能公开使用；无浏览器CORS授权。
- GET /health：protocol_version、稳定executor_id、启动generation与能力，不连接手机。
- POST /prepare：{target,action,pairing_port?,code?}。
- POST /sessions：{id,target,quality,lease_seconds:60}；创建单观看者并保留固定target。
- POST /sessions/{id}/offer：{sdp,type}；返回answer,width,height。
- POST /sessions/{id}/commands：控制消息，返回ack；占用/序号/geometry不符拒绝。
- POST /sessions/{id}/renew：{target_fingerprint,lease_seconds:60}；旧会话或版本不符拒绝。
- POST /sessions/{id}/close：有界清理，保留cleanup_pending。
- POST /runs：{id,expected_executor_id,owner_user_id,actor_user_id,name,steps,parameters,concurrency,targets:[Target]}。
- GET /runs/{id}：返回上述AutomationRun；idempotency journal记录。
- POST /runs/{id}/cancel：{expected_executor_id}；先持久拒收/取消，即使尚未找到run也阻止迟到请求。同身份且确无历史任务时返回fenced:true、never_started:true；已有任务保留实际结果及每设备cleanup_confirmed。
- GET /runs/{id}/artifacts/{artifact_id}：只读取该任务登记的截图，PNG/JPEG，最大8MiB。

Target字段：device_id,name,owner_user_id,machine_id,ip,adb_port,connection_mode,target_fingerprint,route_context:"default"。只允许IPv4单目标、有效端口；localhost/广播/组播等拒绝。首版支持IP路由接入，不另开USB资产自动导入。

执行器向主服务POST /api/v1/automation/internal/authorize（Bearer共用secret），{run_id,executor_id,device_id,target_fingerprint}，响应{authorized:true,lease_seconds:60}；无权/记录不存在/取消/目标变更拒绝。每步骤前查询，网络失败停止新动作并记录中断。
执行器向主服务POST /api/v1/automation/internal/events（同认证），{event_id,executor_id,run_id,result:AutomationRun}；主服务核对执行器身份、归属、目标快照和版本，只更新匹配设备的状态结果，事件去重。发送失败留journal，用户refresh补取，不自动重放任务。

明确的automation准备动作启动受管的长期uiautomator2 helper，维持手机辅助服务生命周期；脚本步骤在独立有界子进程内调用固定版本低层RPC，不使用库内自动重启/重发动作的包装。准备、配对、输入都不回显敏感内容。

## Python媒体适配接口（T04提供，T03使用）

文件automation-executor/automation_executor/media.py，class MediaManager，方法均async：
- create(session_id:str,target:dict,quality:str)->dict（保存单观看者，返回ice_servers）。
- offer(session_id:str,sdp:str)->dict（answer,width,height）。
- command(session_id:str,message:dict)->dict（scrcpy touch/key；中文可交T03动作适配）。
- close(session_id:str)->dict（state,cleanup_pending）。
- close_all()->None。
- status(session_id)->dict为同步只读状态，用于活动租期续验。
T03负责认证、租期、持久控制互斥和任务/观看协调；T04不另做HTTP路由、账号管理或任务引擎。T04必要复用代码放automation_executor/vendor/，包含来源说明，修改EasyADB仓库本体禁止。T03默认脚本走uiautomator2，不依赖观看会话存在。

## 前端文件归属

T05拥有AutomationCenter.vue、components/automation/（除T06三个文件）、modules/automation-api.ts、types/automation.ts。T06仅拥有components/automation/ScriptLibrary.vue、RunHistory.vue、ScriptStepEditor.vue。
ScriptLibrary props：ownerId:number、selectedDeviceIds:string[]、recordedSteps:AutomationStep[]；emits run-created(run)、recording-consumed。RunHistory props ownerId:number、focusRunId?:string；接入方可用key刷新。T06可直接import T05的AutomationApi（默认类）和类型；双方对齐方法接口。
AutomationApi构造器默认无需参数（内部getApiBase），提供context/devices/saveConnection/prepare/createSession/offer/closeSession/controlUrl/listScripts/saveScript/deleteScript/createRun/listRuns/getRun/refreshRun/cancelRun。每方法第一个参数ownerId，context无owner。saveScript(ownerId,body,id?)；body符合上述契约。

## 本轮完成标准与测试边界

代码具备完整设备配置/配对/连接、WebRTC协商、悬浮单控/批量、脚本编辑与运行/取消/记录、可部署执行器及真实错误呈现。不存在默认假数据/模拟成功。未运行测试不宣称可用性已验证。各agent交接给root：修改文件、实际实现范围、未完成项、待测风险，禁止自行测试/构建/部署/commit/push。
