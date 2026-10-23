---
id: REQUIRE-NDS-08
status: discussion
scope: frontend-interaction-review
baseline: feat/node-attributes-main
updated_at: 2026-10-23
---

# 前端经理审查：代码事实与交互边界

承接[需求汇总](../README.md)及已完成的后端 Q01–Q60 问答。前期按用户要求，以严格前端开发经理角度结合实际代码分组讨论交互；前端使用 F 编号，记录在[前端分组答复](require-09-frontend-grouped-decisions.md)。用户限定的最后三题已全部答复，问答结束，本轮只整理文档，不再提问。本文件不是实施计划，也不将建议作为已确认需求。

当前进度：前五组及最后三题均已答复；F02–F15、F19–F21 已确认，F01 已撤回，F16–F18 未答已撤回，不等待、不重问、不记为用户批准。问答已结束，没有待用户回复的问题，不再提出新组或追加澄清。

当前审查读取 `/workspace/EasyTier-main-node-attributes` 工作树源码。Vue 3、PrimeVue 4、Vue Router 4 已在项目使用；无需为此更换框架。源码状态不等于当前部署页面，未启动应用或做浏览器视觉验收。两个 agent 分别只读研究导航/入口和表单/列表，root 核对接口并独占写入本轮文档。

上述独占写入为初始记录；当前协作中，本 agent 仅负责本文件 `require-08-frontend-review.md`，其他文档由 root 负责。

## 已确认规则不重问

- 原网络节点列表、实例数量、设置和实例管理操作保留；新增二级菜单名为“节点详情”。新资料页可用“设备与服务”说明用途和当前节点，不擅自改旧入口行为。
- 节点必选，零实例、离线或零下挂设备均能进入资料管理；新增设备数量与原实例数分开。
- 节点下有直属设备和网关组，设备最多属于同节点的一个组；新增后归属固定，无移动/迁移。组不直接挂服务，自身设备手动创建且每组最多一条。
- 不做聚合；组、设备、服务允许重名；输入、唯一性、网段匹配提示和删除范围按后端已确认规则执行，不重复投票。
- 关键词只搜设备名、型号、组网 IP、内网 IP；节点内组/类型/关键词取交集。最近新增优先，每次查询最新；服务展开时加载，操作后或手动刷新，不新增周期查询。
- 设备/服务新增编辑都用居中弹窗；主动取消、关闭或离开页面直接丢弃未保存内容，不提醒、不持久化草稿。仍打开的窗口保存失败保留输入；关闭或离开不等于取消已发送请求。
- 新增设备始终默认直属节点，需要归组时自行选择网关组，保存后归属固定。新增成功且符合当前已应用筛选时，关窗回列表、定位并展开，方便添加服务但不强制；被筛选排除时自动清空组、类型、关键词并定位展开，节点不变，不增加“查看新设备”按钮，不外推为所有新增都清空筛选。
- 从详情返回原网络节点列表，每次回初始查询、页码和位置，不恢复离开前状态；已有视图、主题和语言偏好保留，不影响 F02/F03 的详情节点恢复规则。
- 普通资料最后一次成功保存生效，同次新增重试不重复创建；非空节点/组禁删，设备确认后删除提交时全部服务。首阶段没有恢复、操作历史、导入导出或手动启停状态。
- 管理员跨账号 CRUD、普通账号自身范围已确认；普通账号间不分享。首阶段普通账号部署下的 Web 管理，external webhook 新功能与程序接入后置。
- 管理员先选目标账号，再选该账号节点，页面始终显示正在管理的账号；后端负责权限检查，资料归属目标账号。
- 服务链接选填，仅支持 HTTP/HTTPS，在新标签页打开，不支持 SSH/RDP 等客户端唤起；服务图标仅用内置图标，未选时显示默认图标，不支持外链图片，不新增图片上传、存储或抓取。
- 本轮关注首阶段资料页面。公网执行后置，不能因讨论前端自行加公开开关、状态探测、转发目标 IP 或强制发布验证。

## 已核对代码与影响

行号用于追溯当前源码，不代表新资料管理已经实现。

|依据|当前行为|新增页面必须处理的区别|
|---|---|---|
|[main.ts:57](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/main.ts:57)、[DeviceList.vue:97](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:97)|旧 `deviceManagement` 是节点 ID 和可选实例 ID 驱动的嵌套路由；列表根据 `deviceId` 打开抽屉|新资料详情路由不能意外触发旧实例管理抽屉。路由名称、参数编码由工程处理|
|[MainPage.vue:162](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:162)、[MainPage.vue:179](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:179)|工作区有仪表盘、网络拓扑、网络节点，没有新资料详情项；实例管理高亮归原节点列表|新增子项和当前页标识，原父菜单仍能进入原列表，不能把“二级”推导成重写整套导航|
|[MainPage.vue:210](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:210)、[MainPage.vue:237](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:237)|桌面导航与手机 Drawer 分别渲染；已有网络子项用 `child` 缩进|两处入口与权限可见性要一致，不能只改桌面菜单|
|[DeviceList.vue:35](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:35)、[DeviceList.vue:304](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:304)、[DeviceList.vue:344](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:344)|默认桌面表格，也支持卡片；手机另有紧凑列表|这些是右侧内部显示方式，整体仍为左菜单/右功能；F01 表述不清已撤回选择题，既定入口的布局适配由前端承担，不当作用户选择了新增视图|
|[DeviceList.vue:327](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:327)|卡片数字是不可点击的运行实例数量 `span`，设置按钮进入旧管理|新增设备数量入口保留原数值与设置，不替换原动作|
|[DeviceDetails.vue:9](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceDetails.vue:9)、[DeviceList.vue:300](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:300)|`DeviceDetails` 是展示注册节点字段的组件，展开直接用已有对象|它不是新“节点详情”整页，也没有设备下服务的请求、空态或失败重试|
|[NetworkDetail.vue:940](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/NetworkDetail.vue:940)、[NetworkDetail.vue:532](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/NetworkDetail.vue:532)|已有同名“节点详情”运行诊断，离线禁用，读取路由/对等端/ACL并周期刷新|新资料页应明确“设备与服务”用途，不继承诊断页离线限制或轮询|
|[DeviceList.vue:80](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:80)、[MainPage.vue:92](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:92)|原节点列表有 1 秒周期任务，导航有 10 秒刷新|不能把新资产和数量查询直接挂入这些循环；现有机制不因本轮需求而擅自删除|
|[DeviceAttributesDialog.vue:15](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceAttributesDialog.vue:15)、[DeviceAttributesDialog.vue:48](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceAttributesDialog.vue:48)|每次挂载独立草稿，保存中禁止关闭；普通关闭没有脏数据确认|F11 已确认主动取消、关闭或离开直接丢弃未保存内容，不提醒、不持久化；打开窗口内失败保留输入及请求隔离可复用，丢弃输入不等于取消已发送请求|
|[DeviceAttributesDialog.vue:73](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceAttributesDialog.vue:73)、[DeviceList.vue:398](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:398)|项目已有模态表单和右侧 Drawer|网关组管理已定为弹窗；F10 已确认设备/服务新增编辑都用居中弹窗，不搬入现成 Drawer 的运行管理流程|
|[DeviceList.vue:125](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:125)、[DeviceList.vue:383](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceList.vue:383)|旧“删除设备”实际删除注册节点，并可同时拉黑|新下挂设备删除不得调用 `delete_machine`，不得继承“同时拉黑”复选框，确认文案需说明对象层级|
|[NetworkDetail.vue:172](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/NetworkDetail.vue:172)、[NetworkDetail.vue:1277](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/NetworkDetail.vue:1277)|旧 CIDR 控件为输入+标签，旧解析只处理 IPv4，并会为裸 IP 补 `/24`|可参考外观，不能复用旧解析语义；新组要求 v4/v6 标准 CIDR，不自动修正主机位或补网段|
|[UrlInput.vue:35](/workspace/EasyTier-main-node-attributes/easytier-web/frontend-lib/src/components/UrlInput.vue:35)、[UrlInput.vue:74](/workspace/EasyTier-main-node-attributes/easytier-web/frontend-lib/src/components/UrlInput.vue:74)|共享 URL 控件服务于组网连接，缺省协议为 TCP、默认主机可为 `0.0.0.0`|不能直接作为网页访问链接输入；按 F20 独立处理选填 HTTP/HTTPS 链接并在新标签页打开|
|[DeviceAttributesDialog.vue:63](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/DeviceAttributesDialog.vue:63)、[api.ts:240](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/modules/api.ts:240)|属性保存失败内联提示；API 401 触发登录跳转，MainPage 提供回调|新增资料模块要区分字段错误、对象消失、权限变化及保存结果未知；不增加持久化草稿恢复|
|[api.ts:174](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/modules/api.ts:174)、[MainPage.vue:103](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/components/MainPage.vue:103)|`ConsoleInfo` 提供用户名和部署模式，`centralEnabled` 由 webhook 模式判断|该标记不能当管理员身份；跨账号界面须依赖后端明确授权并显示目标账号上下文|
|[console.css:211](/workspace/EasyTier-main-node-attributes/easytier-web/frontend/src/console.css:211)、[i18n.ts:4](/workspace/EasyTier-main-node-attributes/easytier-web/frontend-lib/src/modules/i18n.ts:4)|已有手机布局、中英文资源、主题和 PrimeIcons；当前未见业务图标选择器|继承现有语言/主题和键盘支持；按 F21 仅用内置及默认图标，不增加图片外链或上传；不能宣称完整手机 CRUD 已经具备|

## 已确认交互、最后三题与待定设计事项

本表保留已确认交互及审查记录。最后三题 F19–F21 已全部确认，问答结束；其他未定内容仅为未确认设计事项或建议，不作为待提问计划或待用户回复，不自动进入已确认需求。本轮只整理文档，不再追加任何问题。

|顺序|主题|已定内容或当前状态|代码依据/约束|
|---|---|---|---|
|1|入口与当前节点|F01 内部显示方式问题撤回；保持左菜单/右功能。恢复上次有效节点，刷新/链接只恢复节点，其他条件重置|F02/F03 已确认；F01 不记作用户选择某种视图。浏览器定位不等于跨账号分享|
|2|查询与列表连续性|网关组/类型/关键词统一点击“查询”后生效，输入或选择时不自动更新；只提供“全部设备”或具体网关组筛选，全部含直属与组内设备，不提供“直属节点”单独筛选且保留直属设备登记；多台设备的服务可同时展开，点第二台不收起第一台|F04–F06 已确认；服务仍按需查询、不轮询。返回原列表按 F15 回初始查询、页码和位置|
|3|网关组维护|筛选旁“管理网关组”按钮打开弹窗，管理当前节点全部组（含空组），不增加独立菜单；一行一个网段，可增删行，仍至少一个标准 v4/v6 CIDR；网段不匹配在编辑时提示，保存后在列表内网 IP 旁保留提醒|F07–F09 已确认；提醒不阻止保存、不新增强制确认，不代表在线状态，也不定时检测|
|4|表单与连续新增|设备/服务新增编辑都用居中弹窗；主动取消、关闭或离开页面直接丢弃未保存内容，无提醒、无持久化草稿；新增成功且符合当前已应用筛选时关窗回列表、定位并展开|F10–F12 已确认。打开窗口保存失败仍保留输入，主动离开不等于取消已发送请求；归属新增后不可改，标题和表单必须明确节点/可选组；展开方便添加服务但不强制|
|5|新增归属与列表返回|新增设备始终默认直属节点，需要时自行选组，保存后归属固定；新增成功被当前筛选排除时自动清空组、类型、关键词，节点不变，定位并展开，无需“查看新设备”按钮；返回原网络节点列表每次回初始查询、页码和位置|F13–F15 已确认；本来符合当前筛选时仍按 F12，不外推所有新增必清空。已有视图、主题和语言偏好保留；F15 不影响 F02/F03|
|6|设备类型维护|F16–F18 未答已撤回。筛选旁管理弹窗、设备表单内快捷新增并自动选中、必须明确选择类型，均仅为未批准建议；管理入口、快捷新增方式与必填性仍未定|不等待、不重问、不采纳推荐为批准。类型可新增/改名、在用禁删仍有效；预置清单、共享范围和重名规则仍未冻结|
|7|服务链接与图标|链接选填，仅 HTTP/HTTPS 并在新标签页打开，不支持 SSH/RDP 等客户端唤起；仅提供内置图标，未选时用默认图标，不支持外链图片，不新增图片上传、存储或抓取|F20/F21 已确认。现有组网 UrlInput 不适用；服务名称与打开动作的具体呈现仍为未确认设计事项，不再追加提问|
|8|异常反馈|401 或对象已消失时的错误呈现与结束引导，保留为待定设计事项，不列入提问计划|不复活已删记录、不加版本冲突阻挡；同次新增重试必须复用操作身份而非再建一条。F11 已确认无离开提醒、无持久化草稿；网段警告位置已由 F09 确认，不再提问|
|9|管理员入口|管理员先选目标账号，再选该账号节点，页面始终显示正在管理的账号；资料归属目标账号|F19 已确认；后端负责权限检查，现有部署模式标记不是授权|
|10|移动端|手机 CRUD/类型维护范围及设备、服务的具体呈现，保留为待定设计事项，不再提问或默认全部获批|手机已有独立列表，不能声称具备桌面全部动作；既有响应式、键盘及权限责任保留|

最后三题已答复，本轮问答结束。确定内容与未批准建议分开记录，不再扩展组数、追加澄清或等待其他答复，也不把未定事项中的备选同时纳入首阶段。

表达约束保留：用户明确用“左边菜单、右边功能”描述页面。后续文档围绕点击菜单、选择条件、点击查询、展开服务、关闭表单等具体动作表达；表格/卡片分支、组件和缓存实现不作为用户必须理解的术语。F01 撤回后不再要求用户为内部适配方案投票。

## 前端工程必须自行承担

1. **身份与上下文。** 稳定 ID 驱动路由、选择和更新；重名时补节点/组/IP或短标识。路由仅定位，不授予权限；管理员看到目标账号，普通账号不能靠改 URL 越权。切换账号、节点时隔离缓存和请求，旧响应不能覆盖新对象。
2. **旧入口兼容。** 保留原节点标题/设置/实例入口及运行诊断；新资料页独立路由。原节点与新下挂设备的组件和 API 类型清楚区分，不能因都叫 device 误用删除或配置接口。
3. **数据状态。** 未选择节点、加载中、确实无设备、筛选无结果、请求失败分开表达；错误不能显示设备数量为 0。服务展开有自己的加载、空态、错误及重试，不拖垮整页，不增加服务数量指标。
4. **表单与错误。** 沿用独立表单输入及保存中防重复提交，窗口仍打开时失败保留输入、字段错误就近提示；主动取消、关闭或离开直接丢弃未保存内容，不提醒、不持久化，也不把离开等同于取消已发送请求。CIDR 及地址校验与后端语义一致；非阻断警告不能禁用保存或强制二次确认。
5. **无障碍与安全。** 有标签的输入、可键盘操作的控件、焦点进入/返回、展开状态和明确错误文本属于实现责任。转义名称/备注，校验可点击链接与图标来源，遵守新窗口隔离；不让用户投票决定是否防 XSS。既有中英文与主题继续支持。
6. **查询节奏。** 按已定的操作后/手动查询，不扩展旧一秒轮询。即使新页的数量入口在原列表显示，也应独立安排首次进入、返回和操作后的刷新，不顺带每秒拉资产计数。
7. **权限与部署。** 后端提供可信身份/授权，前端控制呈现但不替代服务端检查；external webhook 首阶段不显示可提交的新资料管理入口。实际部署模式未知/加载失败时不假定有权限。

以上是后续设计与实现责任，本轮仅记录，没有运行检查或声称现有代码已经满足全部要求。
