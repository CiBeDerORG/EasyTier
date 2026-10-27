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

## 2026-10-27 · T11 推送后的自动构建静态类型修复

- GitHub Actions `40465945022` Web 阶段报告 `automation-draft-sync.ts(220,45): TS2367`：当前函数先设置 `state.phase='saving'`，TypeScript 未识别异步任务中对响应式状态的修改，将后续 `state.phase==='unsynced'` 错判为永远不成立。
- 修复：将写入完成后的自动继续排队条件移至独立 `scheduleFollowup(owner,epoch)` 函数，通过最新响应式状态重新判断；不改网络幂等 UUID、CAS、失败状态或数据载荷。
- **未执行**：本地测试、类型检查和手工CI；提交后现有自动 CI 状态独立核对。

## 2026-10-27 · T11 并行提交兼容性补充：保留草稿来源正式脚本

- 在核对 T11 已推送代码（`bad51f51` 与 `ad6ef03d`）时发现：`replaceWorkspace()` 刻意清除了 `workspace.editingId/revision`，这会使已保存正式脚本的未发布改动在刷新恢复后失去原修订来源。
- 补充独立增量迁移 `m20261027_000026_automation_draft_script_link.rs`，为已存在的草稿记录追加 `script_id` 和 `script_revision` 可空字段；草稿保存时校验它们成对出现及来源脚本归属当前账号。恢复时还原正式脚本身份，不触发正式脚本写入。
- 新增前端类型与草稿内容指纹的来源脚本身份字段，网络重试时请求内容与原 UUID 一同冻结；既有已创建草稿的两个字段均为 null，仍可恢复为独立新脚本。
- 未改变草稿和正式脚本之间的权限隔离或 CAS、HTTP 409 行为。对暂存中的不完整步骤和输入文字字面值，后端仍拒绝保存并在界面提示未同步。
- **未执行**：本地单测、Rust/TypeScript检查、lint、手工构建、数据库迁移演练及端到端浏览器测试（调试模式）。仅 push 后通过仓库既有自动 CI 核对构建结果。

## 2026-10-27 · T11 既有构建失败修复

- 自动构建 40466638901 报错 TS2339：`draftFingerprint` 的 `Pick<AutomationDraftWorkspace>` 未包含函数体读取的 `editingId` 和 `revision`。
- 已将入参类型补齐这两个可选字段；不改变指纹内容、CAS 或同步逻辑。
- 本轮未运行本地测试/构建；等待 push 自动触发 CI 验证。

## 2026-10-27 · T12 单步骤试跑设备准备状态机（接手续做）
- 代码：新增 `src/modules/useCardTrialPreparation.ts`。每手机按账号/设备隔离准备状态与并发，核对服务端绑定、占用和预览会话所有权；ADB 断开时仅复用已授权的 `connect`，不做无线配对；UiAutomator2 仅在脚本依赖时初始化，优先查询已有运行中准备任务。
- 安全边界：不调用脚本执行接口，不绕过服务器排他/权限校验；对 POST 超时保留未知状态、仅允许先只读复核，不自动重新提交同类动作；设备切换/取消后过期响应不发布。准备完成仍需 T13 执行资源移交。
- 已知缺口：T14 尚未将此 composable 接入卡片 UI；服务端异步准备实际状态、自动连接权限以及多浏览器占用仍待联调。
- **未执行**：本地测试、构建、类型检查、lint、真机验收；原因：调试模式规则禁止主动运行验证。本次 push 的自动 CI 另行查询，不代表真机通过。

## 2026-10-27 · T13 预览移交与运行/投屏双向独占
- 代码提交：`e4d215b`（服务端新投屏前检查运行任务占用）、`846714a`（仅关闭本页跟踪预览，校验账号归属与服务端清理）、`a7071d0`（自动化中心试跑流程接线）。
- 实现：`create_run` 原有事务式会话禁入不变，新增 `create_session` 反向事务禁入；前端确认仅操作脚本试跑对话框归属的预览、检查本地清理状态，再读取服务端持久化的 active_session / run_occupied。
- 不改 Python Executor：`Runtime._run_device` 仍在动作前采用 `journal.claim` 独占并 `_authorize`；Python 内部预览控制依旧共享 journal 控制表。
- 风险/缺口：T14 尚需把新卡片试跑按钮/结果进度接入；并发事务与多标签页、断线、端口重绑定、会话清理及 FnOS/Android 真机均待联调。浏览器返回准备/关闭成功不等于服务端最终执行成功。
- **未执行**：本地单元测试、Rust 编译、TypeScript 检查、lint、手动构建、实际手机验收（当前调试模式）；push 自动 CI 状态另行核对，不预先认定通过。

## 2026-10-27 · T14 卡片步骤单机试跑与运行进度
- 新增 `useCardRunStatus.ts`、`CardTrialPanel.vue`；扩展 `CardStepTimeline.vue` 增加非条件步骤试跑入口，`PhoneCard.vue` 显示准备/确认/状态面板，`AutomationCenter.vue` 提供历史跳转接线。
- 与 T12/T13 复用：明确操作本页持有的会话，关闭后读取服务端活跃会话/运行占用；T12 再检查绑定与 ADB、根据脚本需要准备 UiAutomator2。实际动作只有在用户额外点击确认后走现有单步 `POST /runs`。
- 步骤副作用确认、条件节点禁止单独执行、输入参数仅一次性使用；执行查询只读，不以“组件就绪/任务已登记”冒充执行或业务成功。
- POST 结果未知时保留原 UUID/正文供人工重查；准备结果未知只能使用原状态的只读复核；页面切换后过期异步回复不能启动新动作。
- GitHub 已按逐项中文提交规则 push；**未执行**本地单测、TypeScript 检查、lint、手动构建、FnOS/Android 真机验收（当前调试模式）。后续以实际对应 CI 结果为准，不将 push 代表部署成功。
- 待解决：T15 失败步骤一键修复/回步；T16 跨机/离开页面保护；真实设备测试仍未进行。

## 2026-10-27 · T15 失败结果原卡片回修 / 人工新试跑
- 新增 `CardRepairEntry.vue`；扩展 `automation-run-repair.ts`（按历史 run/step 生成修复来源上下文、共享草稿修复工作区），`RunHistory.vue`（回原卡片）、`CardTrialPanel.vue`（失败结果直接定位）、`PhoneCard.vue`（步骤回修提示/人工再次试跑）和 `AutomationCenter.vue`（认证核验、正式脚本 revision 比较、草稿 fork、定位原手机）。
- 保留既有“失败后在脚本库修复”路径；新增“失败后在卡片修复”。内联单步骤与当前共享草稿中的完整原步骤 ID+正文一致时不覆盖原草稿；否则创建新的服务端工作草稿身份，避免原服务器草稿/正式脚本被修改。修复后新运行必须重新由用户确认。
- 危险边界：`outcome_unknown` 需要用户确认人工核对，普通时间线试跑按钮受当前回修上下文限制；服务端占用锁仍会阻止清理待确认的新运行。历史步骤证据只读，不自动修改旧 run/step/result。
- **未执行**：本地单测、类型检查、lint、手动 CI、Rust/Python 编译、浏览器与 FnOS/Android 真机。原因：当前调试模式。新 push 自动工作流只检查对应版本，需独立确认结果。

## 2026-10-27 · T15 复核与未知结果提醒保护
- 针对 T15 回修横幅的 `outcome_unknown`，新增关闭提醒前的人为核对门禁；普通时间线试跑入口在回修上下文存在时不能绕过“再次验证”确认。该前端提醒并不代替 Rust 服务端独占/清理门禁。
- GitHub Actions `40491745190` 对应代码提交 `27aba9be`：查询到 **Build Web and config generator = success**、**Prepare native programs (amd64) = success**。当时 Package 和 Native regressions 仍在执行，不能据此断言完整 CI 或 FnOS/Android 实测成功。
- 新的提醒保护提交 `f862eba` 晚于上述 CI，尚未得到该提交对应的构建验证；**未执行**本地单元测试、vue-tsc、构建、lint、真机验证（当前调试模式）。

## 2026-10-27 · T16 跨手机复用与页面/账号切换保护
- 新增 `automation-device-compatibility.ts`；修改 `CardTrialPanel.vue`（卡片试跑、提交前只读双重核验），`ScriptLibrary.vue`（单机/批量逐设备只读校验和风险确认），`AutomationCenter.vue`（当前拾取设备显示、浏览器和 Vue Router 离开保护）。
- 坐标步骤严格比较参考宽高与目标设备的只读控件快照，尺寸/方向不一致 **阻断执行**、不做自动缩放。选择器/XPath/滚动容器的跨手机正确性只能提示人工确认，不能将只读匹配冒充真实执行结果。纯 native ADB 步骤无需读取 UiAutomator2。
- 已确认保存的草稿不会因为关掉 WebRTC、切内部 Tab/打开其他授权手机而清空；未同步的修改在本页保留，浏览器刷新/离开 SPA 页面必须明确确认。草稿由 T11 通过服务端 CAS 管理，网络未知使用相同 request_id。
- **未执行**本地单测、TypeScript 编译、lint、手动构建、FnOS/Android/多浏览器实测（调试模式），自动 CI 结果仍需按对应 commit 核对，不能宣称 AC-11/AC-12 已验证。

## 2026-10-27 · T17 三层自动测试补齐（调试模式）
- 新增三个前端 Node 测试文件：`automation-card-workflow-t17.test.mjs`（AC01–08/12）、`automation-device-compatibility-t17.test.mjs`（AC09/11）、`automation-run-safety-t17.test.mjs`（AC09/10）；追加到原有 `pnpm --dir easytier-web/frontend test:unit` 脚本。保持默认 runner，不引入 Vitest。
- 新增执行器独占与 SQLite 恢复测试 `automation-executor/tests/test_trial_handoff_t17.py`；Rust 后端 `drafts.rs::t17_draft_regressions` 测试授权隔离、幂等回执、UUID/CAS 修订校验、输入敏感值保护。
- 为原生 Node 测试解析，调整 `automation-device-compatibility.ts`、`automation-pick-engine.ts`、`card-step-projection.ts`、`automation-card-step-edit.ts` 的纯本地模块导入扩展名为 `.ts`；无业务分支变更。
- **CI 真实结果：** GitHub 自动工作流 `40495290119` 的前端 test:unit 首轮为 125 项、124 项通过、1 项失败；失败点 `AC12 disconnect/switching viewed phone` 用整个步骤对象比对，而 `appendAutomationDraftSteps` 在对象上补充了两个 `undefined` 字段。已提交 `3d6b6d3` 改为比较真实需保持的步骤 ID/动作，不修改生产业务代码。
- 之后自动 CI 执行中；`40495449470` 仅证明对应 executor 构建/烟测成功，不代表新 Rust 用例或最后前端代码全面验证成功。完整对应运行结果需要另行核实。
- **未执行：** 本地 Node/Python/Rust 测试、手动触发 CI、FnOS/Android 真机、浏览器鼠标/触屏/全屏/SPA 实际交互测试。当前处于调试模式。
- 后续必须做一次带真实设备、浏览器和服务端的 AC01–AC12 端到端验证，尤其 T10/T11 真实 CAS、多页面争用、T13 真实执行器 handoff 与 T16 退出守卫；不能把模拟测试当作已验收。

## 2026-10-27 · T18 FnOS/Android/浏览器实机验收的可执行操作卡
- 创建 `task/acceptance-report.md`，覆盖 AC-01～AC-12 的前置环境、逐步操作、通过标准、记录 `run_id + device_id + step_id` 的证据槽位；包含小米11/另一 OEM 的真实信息录入位置、桌面右键/触屏长按、视频黑边/全屏、连续四步、UIA旧树、ADB掉线、草稿 CAS、跨用户授权、多浏览器抢占、执行器恢复/未知动作的10项负面场景。
- 本轮根据已存在的 GitHub Actions 工作流日志确认：`40495705617` / `152eeea688cc2b6e4034deb0708fd5a5f387c91b`，`Build Web and config generator` Job **success**，`test:unit` **125 tests、125 pass、0 fail**。此结果只能用于该提交的前端代码与 Node 单测；完整 GHCR 发布/Python/Rust测试/真机无证据，不能标通过。
- **实际未执行：** 远程 FnOS 登录、自动化中心浏览器点击、ADB操作、UiAutomator2初始化、手机触控、模拟网络中断、修改草稿、执行单步试跑及真机两个浏览器竞争。原因：当前调试模式没有授权主动测试/部署真实环境，且没有本轮已确认的现场测试权限。
- **当前验收判定：** 12/12 项状态为`未执行`，没有真实机型/镜像Digest/运行编号与截屏证据，T18 只达到“准备就绪/待实测”，不是“已验收”。

## 2026-10-27 · 审核 P0～P2 代码修复（调试模式）
- 基线 `765fe64`；修复依据 `task/review-fixes-2026-10-27.md`。审核提出的七项问题均已有增量补丁：卡片 POST 身份保留（P0-01）、正常动作结果展示（P1-01）、只读准备恢复时重新校验（P1-02）、异步草稿恢复不覆盖后续本地修改（P1-03）、执行器坐标宽高一致（P1-04）、拾取外右键不拦截（P2-01）、卡片正式发布与服务器草稿同步分离（P2-02）。
- P0 扩展到脚本库的单机与多机运行：同一 UUID 按目标手机保存请求身份，保存在当前浏览器标签会话的 sessionStorage，内容**只有** ownerId/deviceId/stepId/requestId/createdAt；没有脚本正文、敏感输入、令牌或密码。旧请求未确认时阻止新请求；只读查找执行历史且匹配 UUID+设备后才解除保护。旧回执未被近期 100 条历史包含时仍保持禁止自动重新执行的状态。
- Rust `runs.rs` 只读 run JSON 新增 `request_id`，支持无 POST 的断线恢复；保留数据库事务锁与 idempotency hash，不改执行器排他逻辑。
- 新增 `automation-trial-receipt-regression.test.mjs` 与 `automation-run-result-regression.test.mjs`，纳入现有 `test:unit`。生产环境的跨手机高风险操作仍须独立确认。
- 已有 GitHub Actions `40498979066` 在 `5b39f1dc` 版本成功构建 executor，但**晚于该提交的其他代码**和新增测试不由此结果覆盖。**当前最新整合提交的 CI 结果待核实。**
- **未运行**本地单元测试、TypeScript检查、lint、手动 GitHub workflow、FnOS/Android 真机验收（当前调试模式）。不可把“已提交代码”记为已关闭问题或通过 12 项 AC。

## 2026-10-27 · 单画面与可视化试跑需求记录（仅文档）
- 本轮核查 `PhoneCard.vue`、`AutomationCenter.vue`、`UiInspectorDialog.vue`：控件弹窗复用同一 `MediaStream`，但使用第二个 video 元素；未来改为卡片唯一原视频和右侧属性停靠面板。
- 核查 `CardTrialPanel.vue::prepare()`、`automation-run-handoff.ts`、`automation-executor/automation_executor/runtime.py` 与 `journal.py`：前端在准备前主动关闭本页预览；独占来自 EasyTier 自研 `Journal.claim()`，不是第三方框架。视频 session 创建自身不直接 claim，手动触控/解锁才 claim manual，准备和脚本又分别 claim prepare/script。保留视频必须实现只读观看与写动作分权，并处理 Rust server 占用状态。
- 记录试跑去掉冗余定位风险大卡/额外复选框，但执行器与服务端严格匹配及执行确认不取消；全部新需求见 `require/require-04-single-screen-observable-trial.md`。
- **本次未执行任何代码修改、测试、构建、CI 或设备操作。** 新需求仅状态为“待开发/待真机验证”，不改变原 AC01–AC12 当前验收状态。

## 2026-10-27 · 产品经理最终交互需求收敛（仅需求文档）
- 用户新截图指出卡片时间线 `▶` 应点击后直接在**本卡片绑定的手机**自动准备并执行当前步骤，不再反复打开单步骤试跑弹窗；动作可在**原实时画面**观察。真正不可执行时硬阻断，结果未知不得重放。
- 共享自动化草稿区域增加全局 `▶ 运行全流程`，仅在工作台勾选且授权的手机启动完整脚本，按设备并行、设备内部顺序执行。
- 当前脚本选择器下拉第一行固定 `输入新脚本名 + 创建`；已有脚本支持**选中打开**和**克隆副本**两种操作，包括当前脚本。选中/克隆不覆盖未同步编辑；克隆新身份、原正式脚本和执行历史不变，不保存运行参数秘密。
- 卡片脚本步骤列表增加删除，复用 `ScriptLibrary.removeStep` / `removeFlowBlock` 的分支安全规则与草稿 CAS；结构错误不可删除。
- 已将上述与前一版 `require-04` 的“原卡片唯一实时视频/试跑无重复定位风险确认/执行时只读视频观看和动作独占”统一收敛至 `require/require-05-final-card-script-workflow.md`，包含 FINAL-01～08 开发包与 F-01～F-12 验收标准。
- **本轮未修改业务代码、未执行测试、类型检查、构建、手动 CI、浏览器或手机动作**（仍处于调试模式）。所有 FINAL 项均**待开发**；用户原 AC01～AC12 真机验收仍未执行。

## 2026-10-27 · 脚本下拉框最终细化（仅需求）
- 严格限定控件宽度到截图红框内现有“自动化草稿／智能家居巡检”所在的左侧名称区域，保留右侧 `1/100 步` 与收起/展开按钮的原布局，不改手机画面、卡片和步骤区域宽度。
- 收起时名称显示当前脚本。展开时浮层从左侧原名称位置开始，不额外展示一个重复的当前脚本名称/标题；首行直接“输入名称 + 新建”，下方列表用勾选唯一表示当前脚本，各行保留“克隆”。
- 补进 `require/require-05-final-card-script-workflow.md` 的页面交互规则及 F-13 验收条件。
- **未修改业务代码；未执行测试、构建、类型检查、CI、浏览器或真机操作**（调试模式）。

## 2026-10-27 · 全步骤删除范围确认（仅需求文档）
- 需求细化：**脚本卡片下方每一行步骤**（包括点击、输入、等待、断言、截图、条件分支节点和分支子步骤）都提供独立删除按钮，与 ▶、编辑并列；不得只对普通步骤提供删除。
- 普通步骤直接从当前草稿删除、支持短时撤销；删除条件节点前列出受影响范围并确认；删除分支末尾一步若会破坏条件结构，删除按钮仍须可访问并给出“删除所属条件块”等安全处理途径。
- 更新右侧步数、后续序号、草稿 CAS 状态，不改其他步骤 ID、不修改已提交的执行快照与历史。
- 更新 `require/require-05-final-card-script-workflow.md`、`task/README.md`；**本轮没有修改业务代码、没有执行测试/构建/CI（调试模式）**。

## 2026-10-27 · FINAL 产品需求 V2.0 重整（仅文档）
- 重写 `require/require-05-final-card-script-workflow.md`，将旧版混合的脚本管理/执行器改造明确分为 **A 阶段红框内局部修改**、**B 阶段运行与单视频专项**，消除对“先只动红框”的范围误解。
- A：左侧原位脚本下拉，右侧 `x/100 步` 与折叠永不遮挡；打开第一行立即新建，已选脚本仅在列表勾选不重复顶栏；选中加载正式脚本；克隆须改名确认并创建新独立记录；每一条步骤均有 `🗑`（条件/分支步骤也有）且结构安全/CAS/撤销；全流程入口只允许在红框状态行紧凑展示。
- B：绿色 ▶ 直接在本机真实执行、完整多设备执行、同一 WebRTC 视频不关闭、viewer/action 权限拆分、Inspector 单画面 dock 等，保留需求但不带入 A 阶段的业务代码修改。
- 新版使用 **R01～R08** 业务规则与 **A-01～A-10 / B-01～B-06** 验收，保留原 AC01～AC12；任务看板 `FINAL-01～08` 按 A/B 对齐。
- **只更新 GitHub 文档，无业务代码改动；未运行测试/构建/类型检查/lint/手动 CI/实机验证（调试模式）。FINAL 仍待开发。**

## 2026-10-27 · FINAL V3 用户最新新增要求（只记录需求）
- **最高约束**：所有自动化操作都必须基于自动化中心手机卡片列表的该设备**同一个主实时画面**，不能为了脚本编辑/克隆/单步/批量/控件拾取切到新视频或独立预览，原视频不停。
- **布局替换**：把脚本模块和步骤列表从卡片底部常驻区搬进可拖拽“自动化脚本”主悬浮窗；内部步骤编辑、克隆命名、条件删除、参数输入及必要属性/运行反馈也用可拖动子窗；不复制第二套脚本数据。
- **icon**：手机悬浮工具栏在原唤醒按钮槽位放新增自动化按钮，唤醒按钮上移，原功能不变；自动化按钮开/关浮窗，关闭不丢已确认草稿、不停止视频或后台脚本。
- **全屏兼容**：PhoneCard 目前对 `phone-screen` 调用 Fullscreen API，现有编辑器位于该容器外；未来主/子悬浮窗必须在 fullscreen top layer 内或受控 Teleport，鼠标/触摸拖动、滚动、按键、保存、步骤执行/右键拾取均可用，不必先退出全屏。
- **保留前述确认**：脚本下拉窄宽、展开首行新建不重复标题、选中加载、克隆改名产生新 ID、每步都有删除、单步本机 ▶、全局选机 ▶、viewer/action lease 写锁安全不降低。
- 主文档 `require/require-05-final-card-script-workflow.md` 重整为 V3.0；FINAL-09/FINAL-10 纳入任务看板，验收改为 FF01～FF18；旧 AC01～AC12 保留。
- **本轮业务代码未修改；未测试、构建、类型检查、手动触发 CI 或真机操作**（调试模式）。新增需求均未实现且未验收。

## 2026-10-27 · 产品前端交互收敛与 R0 代码去重（调试模式）
- 新增 `require/require-07-product-frontend-simplification.md`，以 FINAL V3 原实时画面为准，列出卡片 footer/ScriptLibrary 双 UI、Inspector 二次 video、脚本库 run preview、两个试跑弹窗、跨 Tab 编辑等冗余，按 R0～R5 明确替代后删除门槛。
- **实际代码改动仅 R0**：`CardStepTimeline` 步骤正文/编辑图标不再双发 `select/edit`，只发 `edit`；移除 `PhoneCard` `step-select` emit/透传及 `AutomationCenter` 监听。由 `editCardStep()` 单独校验并选中步骤，不动媒体/执行器/脚本 API。
- **未执行**本地 TypeScript、单元测试、lint、构建、手动 CI、FnOS/Android/浏览器交互（调试模式）。待验：编辑时取消放弃未保存步骤，行/编辑按钮触发一次，双手机之间切换编辑状态。其余 R1～R5 均待开发。

## 2026-10-27 · FINAL V3 正式需求计划与任务计划拆分（纯规划）
- 在 `plan/` 新增 **P07～P16 共 10 份独立小需求计划**，另新建 `plan/README.md`：唯一实时画面合同、全屏主子浮窗、空脚本新建/克隆/CAS、全部步骤删除、Inspector 单视频、viewer/action 租约、单步、全局运行、旧 UI 安全清理、QA。
- 在 `task/` 新增 **T19～T51 共 33 份可执行小任务**，另新建 `final-v3-task-index.md`，明确每项 dependencies、文件写入 Owner、实施步骤、风险与验收。原 T01～T18 不改状态。
- 真实代码已查证 Rust validation::steps 当前拒绝空 steps，前端 ScriptLibrary::validationError 同样只允许 1～100 步。已安排 T25 专门支持**创建/保存 0 步**，执行依旧拒绝 0 步，不取消权限/版本检查。
- 已明确 PhoneCard/AutomationCenter/ScriptLibrary 等热点文件的 Agent 独占，跨前后端并行只在文件不冲突时进行；优化时遵守“替代已验证→删除旧层”。
- **本轮没有修改业务代码、未执行单测、类型检查、lint、构建、手动 CI、FnOS/安卓真机操作（调试模式）**。全部新增任务待开发，验收未执行。

## 2026-10-27 · T19 基线审计（调试模式）
- 对照分支 `91392fd` 的 PhoneCard、CardStepTimeline、CardStepEditor、UiInspectorDialog、ScriptLibrary、CardTrialPanel、PhoneAutomationPanel、RunHistory、AutomationCenter 与运行 handoff、脚本校验，整理 `require/require-08-t19-baseline-audit.md`。
- T19 已完成代码级审计与旧入口替代门槛矩阵，**无业务代码修改，无 TypeScript/测试/构建/CI 手动触发/真机验证**；后续 T20 与 T21 不能把 T19 当作新功能验收通过。
