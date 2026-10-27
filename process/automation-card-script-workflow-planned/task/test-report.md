# 本需求测试与验证记录

## 2026-10-27 · 需求/计划/任务拆分
- 依据用户确认的12条业务验收要求；审阅 `feat/automation-center` 现有卡片、Inspector、脚本库、共享草稿、准备与执行记录相关代码。
- 产出：3份需求合同、6份里程碑计划、18个小任务、任务看板；没有改变生产代码。
- 执行结果：**未执行**构建、测试、CI、真机操作；原因：本轮仅要求产品与开发任务拆分。
- 现状：所有新增任务待开发，12个验收标准尚未逐项通过。
- 特别风险：异步锁移交竞态、旧树坐标不一致、全屏 top-layer、服务端草稿版本冲突、不跨账号写入、自动化脚本副作用。

## 2026-10-27 · T01 拾取状态与共享草稿接线
- 完成：新增纯状态模块 `automation-picker-state.ts`（关闭/准备/开启/错误、append/replacement、单调世代、快照版本、绑定当前拥有者/会话/目标指纹/脚本修订、过期结果保护），并在 `AutomationCenter.vue` 维护每台手机独立状态。
- 完成：旧控件拾取入口接入状态基础合同；ADB 会话退出、卡片关闭、换账号、设备重新绑定、换脚本修订、切页面、启动群控或录制时关闭拾取；方向变化时版本失效。已确认草稿步骤不随拾取模式清理。
- 尚未实现：T02 图标真正开关/展示状态，T03 原画面右键菜单，T04 命中引擎，T06 异步拾取 token 与新增去重。现有 Inspector 暂时保持原 UI，继续按旧方式点选。
- 已知风险：M3 持久化草稿尚未实现；当前页面刷新时未保存草稿不能恢复；旧 Inspector 仍不回传快照世代，因此端到端过期结果保护需 T04/T06 补齐。
- **未执行**：单元测试、类型检查、构建、CI 手工触发、FnOS/Android 真机验收。原因：继续遵循当前调试模式，push 后自动 CI 的结果独立确认。

## 2026-10-27 · T02 卡片拾取开关与视频点位映射
- 代码变更：`PhoneCard.vue`（真实开关/状态/右键坐标）、新增 `CardPickingOverlay.vue`、`AutomationCenter.vue` 接线、`automation-picker-state.ts` 补统一的 `PickerVideoPoint` 合同。
- 新行为：原画面点击图标开启/退出，视频左键继续真实控制；右键仅传递视频真实坐标，黑边不采集；临时复用旧 Inspector 检查窗口，**没有**自动命中和右键操作菜单。
- 全屏过渡：旧 Inspector 仍挂在 body，右键时先退出全屏再显示旧弹窗；后续 T03/T05 处理不打断全屏的菜单与面板。
- 代码状态：已提交，**待联调**，不宣称 AC-01/AC-02 已通过真机验收。
- **未执行**：本地单测、TS 检查、构建、手机/浏览器验收；当前继续调试模式。push 后自动 CI 状态另行核实。

- T02 接线补充：确认前端 tsconfig 开启 `noUnusedLocals`，保留旧 Inspector 入口但由右键实际调用；已经开启拾取时不重复推进 generation，防止已捕获的坐标立刻过期。未执行本地检查。

## 2026-10-27 · T03 原画面右键快捷菜单及触屏长按
- 修改 `PhoneCard.vue`：右键不直接弹 Inspector，而在当前视频卡片内开菜单；用原有真实像素映射，排除 letterbox 黑边；引入 touch 约550ms hold 缓冲，把拾取长按与 Android 真实手势区分，触屏短按/滑动照常执行。
- 新增 `CardContextMenu.vue`：菜单含控件树/刷新/配置点击长按输入等待断言/退出，Esc、方向键、Home/End 与全屏内部覆盖；点击画面其他位置关闭。
- `AutomationCenter.vue` 接线，菜单选项保留 `PickerMenuAction` 和右键坐标与 generation；刷新只读 hierarchy API。非刷新项进入现有 Inspector 并提示选择对应动作，不会假装自动识别或自动生成步骤。
- **待联调**：T04控件树坐标命中、T05预选动作/定位候选、T06确认录入后自动收窗保持拾取，以及浏览器全屏菜单与移动端长按的实际交互。
- **未执行**：单测、TypeScript检查、构建、lint、FnOS/Android和浏览器真机验收（继续调试模式）；未手动触发 CI，push 后自动构建状态需单独核实。

## 2026-10-27 · T04 控件树快照与原画面右键位置匹配
- 代码：新增 `automation-pick-engine.ts` 纯函数，以设备指纹、捕获时间、视频/快照分辨率和点位联合校验；拒绝过期、方向错误、控件树截断、重复节点编号、超出内容区域的点击；排序命中节点并返回可点父节点及唯一定位候选。
- 修改：`UiInspectorDialog.vue` 内画面选择共用命中函数，来自原卡片的首个右键点位会在新层级读取后自动预选控件；`AutomationCenter.vue` 只在世代、会话及草稿上下文仍一致时转交坐标。
- 安全：纯只读，不执行 Android 触摸；已命中不等于定位已确认。未自动采用含糊候选/坐标，密码控件不展示敏感原文。
- 后续：T05 在菜单动作与 Inspector 属性面板之间建立默认选择及安全操作合同；T06 完成确认关闭属性窗和连续追加去重。
- **未执行**：本地单元测试、TS 类型检查、构建、lint、FnOS/Android 真机验收（调试模式）；提交自动触发的 CI 另行检查，不宣称验证通过。

## 2026-10-27 · T05 共用 Inspector 面板与右键操作类型预选

- 范围：新建 `InspectorPropertiesPanel.vue`（原控件树、属性、候选定位、坐标人工确认、动作按钮抽成复用组件）；新建 `automation-inspector-actions.ts`（右键菜单动作 → Inspector 动作受限映射），`UiInspectorDialog.vue` 通过 props/events 连接原状态，`AutomationCenter.vue` 传入与拾取会话匹配的右键首选动作。
- 保留：UiAutomator2 快照独立读取，T04 命中控件预选，人工点击候选确认后才能添加；敏感输入框文字/描述继续隐藏；已有脚本库重拾只允许替换原目标，不作为卡片“新增”。
- UI：右键指定操作会突出显示“确认录入 · 操作名”主按钮，其他动作仍可切换；匹配不唯一/控件树截断时禁止录入，手动坐标定位仅用于点击/长按。
- 待续：T06 实现确认后关闭属性窗而保留卡片拾取开关、异步世代令牌与重复提交保护；T07 再实现步骤时间线。
- **未执行**：本地测试、Vue/TS 类型检查、构建、lint、浏览器与 FnOS/Android 真机操作（沿用调试模式）；仅 push 触发用户原有自动 CI，不能据此声称真机验收通过。

## 2026-10-27 · T06 单窗口一次提交、自动收窗、连续拾取

- 修改 `AutomationCenter.vue`：在选定右键动作及打开属性窗口时捕获冻结的 Inspector Append Scope（设备、用户、目标指纹、会话 ID、草稿脚本/修订、拾取世代/快照世代和弹窗唯一 token）。确认时对原始 scope 与当前状态作二次比对，阻止状态改变或晚到结果误写。
- 修改 `UiInspectorDialog.vue`：新增 `commitToken`，单次交互同步接受/拒绝回调及 `committing` 局部锁；点击过快不会重复提交，服务器/草稿拒绝可恢复点击状态；拖拽步骤也走统一路径，失败不会谎称已录入。
- 修改 `automation-draft.ts`：拾取步骤提交时不自动改写已存在的 ID，实现精确步骤去重。录入成功即关闭 **Inspector 弹窗**，保留手机投屏、拾取模式和共享草稿；提示已成为第 N 步并可继续右键录入。
- 修复 T05 原有构建失败：GitHub Actions `40458079627` 的 Web 编译错误为 `UiInspectorDialog.vue(476,21): TS1003 Identifier expected`，原因是 T05 模板抽取遗留半截 `<small v-if=` 标签；恢复原视频方向提示与闭合节点。
- **未执行**：本地单测、TypeScript 检查、lint、构建、Android/浏览器真实连续右键四步验收（调试模式）；本轮提交所触发自动 CI 需后续独立确认。
- 仍有后续：T07 步骤时间线常驻卡片、T10/T11 服务端草稿持久化；未保存的前端草稿刷新页面仍有丢失风险。

## 2026-10-27 · T07 手机卡片共享草稿步骤时间线

- 变更：新增 `CardStepTimeline.vue`（折叠/步骤列表/选中态/临时的脚本库编辑入口）和 `card-step-projection.ts`（单源数据的只读摘要、保存状态、条件分支层级）；`PhoneCard.vue` 直接在视频区下方挂载，`AutomationCenter.vue` 通过原 `draftWorkspace` 下发及处理选中事件。
- 产品：录入即显示第N步，当前启用拾取的卡片可自动展开，其余卡片折叠，详细步骤内部可滚动；点击步骤同步选中，点击铅笔进入已有脚本库，避免复制编辑器。显示条件 Then/Else 范围、文本类参数仅显示类别不暴露实际值。
- 状态：对照脚本库 baseline 只显示「草稿未保存 / 有未保存修改 / 已保存修订」，当前无后台草稿同步功能，绝不提示「云端同步成功」。离开当前页面时未正式保存的数据仍可能丢失，交 T10/T11。
- 已知遗留：T08 原卡片内编辑及重新拾取、T09 定位演示尚未开发；同机型实际右键连续4步与手机浏览器展示需阶段联调。
- **未执行**：本地单测、类型检查、lint、构建、FnOS/Android/浏览器实际操作（遵循调试模式）；仅推送触发原有自动 CI，结果另行核实。

## 2026-10-27 · T07 集成期修正既有 CI 编译阻断

- 核查先前 T06 push 对应 GitHub Actions `40458883740`：Web 资产阶段 `vue-tsc` 因 `UiInspectorDialog.vue(489): TS2345` 失败，详情为子组件缺少 `suggestedXPath` 必填属性。
- 原因：T05 属性面板拆分后，父组件使用 Vue 自动解析的 `suggested-xpath`（对应 `suggestedXpath`），子组件却声明驼峰缩写 `suggestedXPath`，TypeScript 严格组件入参校验不匹配。
- 修复：`InspectorPropertiesPanel.vue` 统一使用 `suggestedXpath` props 及插值，与父组件既有绑定匹配；不改变 XPath 构造逻辑或控件定位条件。
- **未执行**本地测试、类型检查、构建、手工 Actions/CI；只对已有 Actions 失败日志做复盘，最终结果以后续 push 自动 CI 为准。

## 2026-10-27 · T08 卡片步骤原位编辑及重新拾取

- 变更：新增 `CardStepEditor.vue`（复用 `ScriptStepEditor`，编辑副本暂存/保存/取消/重新拾取）、`automation-card-step-edit.ts`（按 stepId/CAS/类型/条件跨度校验安全替换）；`CardStepTimeline.vue` 点击步骤直接打开卡片编辑器，`PhoneCard.vue` 承载编辑器及事件。`ScriptStepEditor.vue` 新增可选动作类型锁定开关，不影响脚本库原操作。
- `AutomationCenter.vue`：同一账号/当前设备的共享草稿编辑；切换步骤、关闭编辑或切换页时，若参数未保存，会提示放弃。重新拾取设置 `PickerIntent.replace`，下一次右键预选控件；确认复用 `replaceAutomationStepTarget`，保留原 ID/动作参数/步骤位置/条件范围。
- 安全：读取右键后的新控件树；替换前校验当前设备会话、target fingerprint、draft revision、拾取/快照世代、打开窗口时的步骤 JSON 未被异步修改；老 Inspector 脚本库路径保持独立；失败/取消不写草稿。
- 仍待联调的场景：手机横竖屏、旧 UIA 快照、正在编辑时投屏掉线、全屏右键退出顶层、不同浏览器同时编辑；T09 的只读定位演示未开发，T10/T11 服务端草稿持久化未开发。
- **未执行**：本地单元测试、TypeScript 类型检查、lint、构建与 FnOS/Android 真机操作（遵循当前调试模式）。仅按分支 push 触发既有自动 CI，最终状态另行确认。

## 2026-10-27 · T09 卡片只读定位演示与单控件高亮

- 新增 `automation-locator-demo.ts`：针对新鲜UiAutomator快照计算 Resource ID / text / description / class / package 的组合条件精确匹配数，安全分辨 0/1/N；对过期/截断/机型方向/设备绑定错误返回不可预览，不假装唯一匹配；XPath 因无执行器原 XML 不提供伪造的前端 XPath 结果。
- 新增 `CardLocatorDemo.vue`：原手机画面内只读 SVG 轮廓，按视频实际尺寸 + preserveAspectRatio meet 计算，避开 object-fit contain 黑边；完全 pointer-events:none，Android 不收到演示的任何触摸事件。
- `CardStepEditor.vue` 增加「定位演示（只读）」与结果反馈/清除，`PhoneCard.vue` 呈现覆盖层并转发点击；`AutomationCenter.vue` 仅调用已有授权 GET hierarchy，确认账号、设备指纹、视频会话、分辨率、stepID 与步骤指纹仍未变化才展示。
- 高亮清理：手工操作、切换步骤、保存、重拾、视频失效、旋转、换设备、页切换、运行创建时清除，12秒自动失效；迟到响应不重新显示。
- **未执行**：本地单测、TypeScript检查、lint、构建或 FnOS/Android 真机验证（遵循当前调试模式）；push 后原有自动 CI 状态应单独核验，未宣称验收成功。
- 后续：T10 服务端草稿持久化、T11 前端同步；M4 正式单步骤执行与失败回修。

## 2026-10-27 · T09 提交后的 TypeScript 编译问题针对性修复

- 核查 T08 CI `40460773164` 与 T09 CI `40461701630` 的 Web 构建日志：`AutomationCenter.vue` 分别报 TS2339 `PickerIntent` 的 `stepId` 属性不存在；此前 T08 引入的 `current.intent` 在异步 Inspector 分支内未能被 vue-tsc 保持 discriminated union 缩窄。
- 修复：局部冻结 `capturedIntent: PickerIntent`，先判断 `kind`，仅在 `replace` 分支读取 `stepId`。不取消设备与草稿的 generation 二次校验，不改步骤新增/替换逻辑。
- **未执行**本地测试、类型检查、构建或额外手工 Actions（调试模式）；本次修复随正常代码 push 触发已有 CI，真实成功状态需另行核验。

## 2026-10-27 · T10 服务端草稿数据、API、CAS及请求幂等

- 数据迁移：`m20261027_000025_automation_drafts.rs` 新增草稿表（owner/actor/ID/revision/时间/步骤）及独立操作回执表（owner+actor+request_id 唯一）；不更改已有正式脚本表、运行表及执行协议。
- 数据访问：`db/automation/drafts.rs` 提供按owner列出/读取/新建/按修订更新/按修订删除，写事务采用 `BEGIN IMMEDIATE`，写入前二次检查账号授权；404 表示当前账号内不存在，409 表示草稿修订冲突或同UUID重放内容不一致。
- 保护：每次新增/更新/删除保存请求 UUID+hash及当时的结果作为原子回执，网络重试不会重复新建或增加 revision；删除后仍可重放同一删除请求并获取当时精简回执。仅在草稿表储存步骤正文，幂等回执表只保存 ID/修订号/删除标志，避免删除后残留正文。草稿空步骤合法，非空步骤复用后端正式脚本格式/分支校验；输入文字步骤只允许参数名，禁止保存字面输入及运行时参数值。
- API：`GET/POST /api/v1/automation/drafts` 与 `GET/PUT/DELETE /api/v1/automation/drafts/{id}`，全程已有 session 登录、origin/actor/owner-scoped 认证；字段/状态/重试语义写入 `task/t10-draft-api-contract.md`。
- 限制：此轮**只有服务端**，尚无前端定时同步、断线恢复或版本冲突提示；这些是 T11/T16。已经确认并写入服务端的步骤不会因视频断线丢失，但尚未接入前端的步骤仍仅在内存。
- **未执行**：本地 Rust 测试、构建、类型检查、迁移演练、FnOS/SQLite 实际部署或接口调用（继续调试模式）；提交触发的原有自动 CI 状态另行核实，不能据此宣称代码已验证。

## 2026-10-27 · T10 推送后发现的既有 T08 编译阻断

- 复核历史 GitHub Actions `40461898967`：`AutomationCenter.vue(1064,83)` `TS2339 PickerIntent.stepId`，T09 上次修复只覆盖右键菜单提交分支，**未覆盖 `openInspector()` 的 `Array.find()` 闭包**。
- 修复：对 `pickedIntent.kind === 'replace'` 先冻结 `replaceStepId/replaceStepType` 两个字符串，再进入步骤列表 `find`；不更改草稿和替换步骤操作规则。
- **未执行**：本地测试、类型检查、构建或手动触发CI（遵循调试模式）；新提交自动触发的 CI 需单独确认。

## 2026-10-27 · T11 前端草稿自动同步/恢复及脚本库交互

- 新增 `automation-draft-sync.ts`：单例 owner-scoped 服务器工作草稿同步管理，直接监听唯一共享 `AutomationDraftWorkspace`；冻结请求正文和 UUID，使用450ms 防抖、同草稿串行 CAS、失败同请求安全重试；新旧 owner / 已发布脚本身份变化时使迟到请求失效。
- 接口：`types/automation.ts` + `automation-api.ts` 补充 T10 的草稿列出/读取/创建/更新/删除；后端重放回执 `request_replayed` 会 GET 正文并比对修订，避免把其他页面新内容误判为本次确认；HTTP 409 停止覆盖并提示选择人工恢复或新草稿。
- UI：`AutomationCenter.vue` 增加工作草稿状态条、恢复下拉、刷新/保存当前/另存新草稿/删除；浏览器未落库修改拦截 beforeunload，管理账号切换时显式确认；`ScriptLibrary.vue` 原保存按钮改名「保存正式脚本」，展示独立工作草稿同步状态；`PhoneCard.vue`、`CardStepTimeline.vue` 同步显示相同状态，不再把内存保存等同于服务器确认。
- 网络：在线事件或点击「保存当前」重试原 UUID 和原冻结请求，写入响应被确认后顺序保存最新本地内容。未成功时显示未同步；严格校验异常（未填写完整步骤、`input_text.text` 明文被拒绝）会显示错误，不能谎称同步成功。
- 数据风险：尚未落库内容在刷新/崩溃时仍可能丢失；已完成服务端确认写入内容可在重新打开时恢复。工作草稿不会自动发布为可执行正式脚本，试跑需走既有授权运行流程。
- 上游构建：T10 后续提交 `3b9f80c33fbe` 的 GitHub 自动构建最终标记成功，该结果**只对应历史提交**，不证明 T11 通过。
- **未执行**：本地单测、类型检查、lint、构建、FnOS/Android 真机、多标签页 409/断网恢复接口验证（继续调试模式）；push 触发的既有 CI 状态独立记录。
