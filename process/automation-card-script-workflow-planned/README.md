# 自动化中心 · 卡片连续拾取到脚本执行完整闭环

状态：**开发中（T01–T16 业务代码待真机联调；T17 前端 Node 125/125 通过；T18 操作卡已提交、实机尚未执行）**。当前交付分支：`feat/automation-center`。关联现有 `process/automation-center-in-progress/`，不覆盖原需求。

## 新增交互与架构评估（2026-10-27，仅记录）
- **新增需求**：控件拾取与工作台共享唯一主画面、试跑去掉冗余“定位风险确认”展示、自动准备及脚本执行期间保留视频观察且临时禁手控。详情见 [require-04-single-screen-observable-trial.md](require/require-04-single-screen-observable-trial.md)。
- **技术确认**：手机视频本身未被第三方框架禁止与 UIA 共存；当前前端主动停预览，EasyTier 自研 Journal 对手控/准备/脚本的操作锁互斥。建议拆分只读 video lease 与 action lease；安全排他和跨浏览器授权不可删。
- **状态**：需求已记录，未改业务代码，未实测。

## FINAL V3.0：同一实时画面 + 全屏可拖动自动化浮窗（最新）
- **最高优先级、唯一开发基线**：[FINAL V3.0 主需求文档](require/require-05-final-card-script-workflow.md)：全部自动化必须使用**自动化中心卡片列表本机原实时画面**，不许重复手机预览、切独立脚本页或从脚本执行时关闭视频。
- 原卡片 footer 下的“自动化草稿/步骤列表”改为**可拖动的“自动化脚本”浮窗**；所有步骤编辑、克隆改名、条件删除确认及运行输入/反馈也使用可拖动子浮窗，不能遮蔽或丢失原视频。
- **悬浮 icon 位置**：新“自动化”按钮占**原唤醒 icon 槽位**，原唤醒按钮**上移一格**，其它按钮功能和视频尺寸不变。自动化按钮点击开关浮窗；关闭不丢草稿、不停执行。
- **全屏**：浮动工具栏、自动化主浮窗、编辑子浮窗仍显示并可拖动、输入、运行；全屏元素内呈现，不能被 `appendTo=body` 遮住。
- **之前确认的功能保留**：脚本下拉原位窄宽、首行新建、选中加载、克隆先改名并新建独立记录、每步删除、单步 ▶ 本机执行、全局 ▶ 已选手机整套执行、准备运行画面不断、严格写锁排他。
- **任务计划**：`task/README.md` 最新 FINAL-01～FINAL-10；新增悬浮承载 FINAL-09 与全屏综合联调 FINAL-10。**新功能全部待开发**，旧 AC01～AC12 真机验收状态不变。

## 产品与前端代码取舍（2026-10-27）
- [代码取舍审查与分波次清理报告](require/require-07-product-frontend-simplification.md)：先迁移浮窗/主视频再淘汰重复 UI；保留草稿 CAS、只读定位、安全排他和未知结果保护。
- 本轮只做 **R0：卡片步骤不再重复发出“选中+编辑”事件**；旧视频、脚本库及运行弹窗仍未清理，后续 R1～R5 待开发。**未执行测试**。

## FINAL V3 正式小需求计划与开发任务（2026-10-27）
- 需求计划索引：[P07～P16 共 10 份](plan/README.md)。旧 P01～P06 不覆盖，按“状态合同 → 全屏浮窗 → 脚本/步骤 → 单画面 Inspector → Viewer/Action 权限 → 执行 → 瘦身 → 验收”拆分。
- 开发任务索引：[T19～T51 共 33 个独立任务](task/final-v3-task-index.md)，按文件唯一 owner、跨 Agent 依赖、验收条件细分。旧 T01～T18 仍代表历史代码进度与未验收项。
- 重要技术阻断：现有 Rust `db/automation/validation.rs` 对正式脚本限制 **1～100 步**；T25 单列修改**0 步可新建但不能运行**，不能只做前端假记录。
- **执行方式**：P07/T19+T20 先冻结合同；浮窗与后端 viewer/action 租约可并行；每一条旧操作必须有相同能力的浮窗替代后才删旧代码。
- **状态**：本轮只提交规划文档，T19～T51 均未实施/未测试/未构建，继续遵守调试模式。

## 产品目标
一个手机卡片持续完成：打开画面 → 开启拾取 → 右键选择操作 → 查看控件树/属性/定位条件 → 配置并确认 → 关闭属性窗（保持拾取）→ 连续追加步骤 → 编辑/定位演示 → 保存/恢复/跨设备复用 → 自动准备单步试跑 → 查看结果/修复/再试。

## 范围及进度
- 用户已确认的12条验收标准不可擅自删除，详见 `require/require-01-acceptance.md`。
- 需求及异常分支：`require/require-02-user-journeys.md`；技术/安全边界：`require/require-03-architecture.md`。
- 6个可交付里程碑：`plan/plan-01-foundation.md` 至 `plan/plan-06-acceptance.md`。
- 实施任务清单：`task/README.md`；具体操作卡：`task/task-XX-*.md`。
- 既有功能：WebRTC卡片视频、UiAutomator2只读层级、唯一定位候选、共享草稿、脚本库编辑、脚本/单步执行、执行记录及失败修复入口。
- 已接入：前端草稿同步/恢复与卡片内真实单步试跑入口（T11、T14）；T15 历史失败回原卡片编辑和人工重新试跑已接入；T16 跨机兼容核验与切页保护已接入；T17 已增补 Node、Python、Rust 三层测试代码；前端 Node 单测在提交 `152eeea` 的自动 CI 中通过 125/125。T18 已新增 `task/acceptance-report.md` 覆盖 12 项 AC 与10项异常用例，但实际 FnOS/Android/浏览器测试尚未执行。
- 进度：**T01–T16 业务代码已提交，待联调**；T17 三层测试代码已提交，Web 单测 125/125 通过、其余验证另计；T18 验收报告模板已建立、12/12 项未实测。M1/M2 已接入拾取、编辑和只读定位演示；M3 T10/T11 已接入草稿服务端及自动同步；M4 T12–T15 已接入安全准备、资源移交、单步骤执行、结果展示及失败回修；T16 加入跨机尺寸/定位复核及 SPA 离开保护。未完成 FnOS/Android、多浏览器竞争、断网与完整发布验收。
- 顺序：M0基础合同 → M1连续拾取 → M2步骤/演示 → M3草稿保存/复用 → M4试跑回修 → M5综合验收.
- 不在本期：无线调试自动配对、无需用户同意执行有副作用动作、跨设备无条件复用绝对坐标、绕过执行器独占锁、Android完全关机开机。

## 最新代码审核修复
- 已针对 2026-10-27 全局审核的 1 项 P0、4 项 P1、2 项 P2 提交修复与新增回归测试，详见 `task/review-fixes-2026-10-27.md`。
- 卡片单步试跑与脚本库运行现已在 POST 前存储最小化 UUID 请求身份，页面关闭后只读恢复，不持久化脚本参数；草稿恢复有异步变更守卫；执行器严格校验坐标尺寸。
- 修复状态是 **代码已提交、待新 CI 与实机回归**，不改变 T18 的 12/12 项尚未真实验收结论。

## 组织原则
- 多 Agent 并行只发生在**互不重叠的文件**上。集成负责人唯一维护 `AutomationCenter.vue`、`ScriptLibrary.vue` 和共享接线。
- 各模块用 props/events/composables 明确接口交付，禁止复制一套 UiInspector/草稿逻辑。
- 代码按小任务增量 push；用户当前处于调试模式，不主动运行本地测试或手动 CI。真实验收状态严格区别于代码提交。

## 2026-10-28 · P12 / T36～T39 Viewer-Action Lease 开发记录
- 已在 `feat/automation-center` 提交 [T36状态合同](require/require-10-t36-viewer-action-lease-contract.md) 与 T37～T39 Executor、Rust、前端单卡片试跑接线，目标是不关闭原 `PhoneCard.primary-video` 即释放手动写权限。
- Rust/Executor 双边校验后，脚本 `POST /runs` 仅允许同账号/同目标指纹已确认只读观看会话共存；触摸、STOP、输入、解锁后端保留拒写及设备动作独占。
- **尚未完成**：T40 全屏不退场的试跑、T43～T45 批量脚本所有设备原视频观察、T49 后端安全回归、T51 真实 FnOS/Android 验收。不能把 T39 单步接线称为完整 FINAL V3 发布通过。
- 本轮调试模式下**未执行**本地测试、类型检查、Rust/Python 编译、构建、手动 CI、浏览器或真机测试；状态详见 [P12计划](plan/plan-12-viewer-action-leases.md) 和 [测试记录](task/test-report.md)。

## 2026-10-28 · P13 / T40～T42 单步 ▶ 自动准备、结果、失败回修
- 已推送到 `feat/automation-center`：原手机实时画面上的可拖动单步试跑/失败回修浮窗，打开不再退出全屏；单击 ▶ 冻结本机/步骤/UUID，自动准备并在只读无风险检查通过时自动提交，真实操作或有风险的定位要求一次明确确认。
- 一步运行的设备、步骤、请求与 runId 和服务端计时显示在唯一脚本时间线；脚本失败允许从旧只读历史定位本步骤，编辑/重拾并保存修订后用户主动再次试跑，不会修改旧运行记录或自动重发未知操作。
- 单步控件定位使用严格的只读唯一候选核验；目标不唯一、截断控件树、未经验证的 XPath 均硬阻断，风险确认不能绕过。
- **未验收**：浏览器全屏/拖拽、设备旋转、UiAutomator2 多控件/超时、双浏览器竞争、实际 Android 与 FnOS；历史 RunHistory 旧路径仍保留并计划在 T46 后续按替代门槛清理。
- T43～T45 批量脚本同原画面并行运行尚待开发。详见 [P13计划](plan/plan-13-inline-single-trial.md)、[任务总表](task/final-v3-task-index.md)、[测试记录](task/test-report.md)。
## 2026-10-28 · P14 / T43～T45 批量完整流程
- 新批量执行从**工作台手机卡片脚本浮窗**发起，只采用当前勾选设备，同一个 runId/请求 UUID；A/B/C 各自保持原 PhoneCard 主视频及 sessionId，独立准备、并行执行，不为批量任务建副播放器。
- 可从主浮窗查看每机队列、当前步骤、运行/失败/未知、原媒体中断、执行请求与 runId；支持整批停止与单机停止（Rust 新路由复用 Executor 设备级 withdraw），不会把“已请求停止”当作“清理完成”。
- 新 SQLite 兼容迁移保存并发 >3 的实际值，不覆盖历史运行表。单次任务可选设备数目前仍受后端 20 台限制；旧 ScriptLibrary 分屏保留到 T46 替代验证后删。
- **仅代码提交，未进行 CI、浏览器或 FnOS/Android 验收**。查看 [P14任务](plan/plan-14-batch-workflow.md)、[FINAL进度索引](task/final-v3-task-index.md)、[详细测试记录](task/test-report.md)。

## 2026-10-28 · P15 / T46 第二视频及旧试跑重复路径收敛
- 已删除 `ScriptLibrary.vue` 中与卡片原画面执行重复的旧试跑模态层、独立 video 播放器和前端设备预检/重复运行请求；`AutomationCenter.vue` 不再维护旧脚本页预览 `sessionId` 的关闭/转移路径。
- 原 `PhoneCard` 唯一可操作主视频、卡片单步骤浮窗、共享批量运行浮窗、T36～T39 的 Viewer/Action Lease、`RunHistory` 和所有持久动作回执安全约束维持不变；批量新入口会识别历史单步骤未知 UUID 并禁止绕开核验重播动作。
- 旧「脚本」Tab 仍保留正式脚本增改删、分支条件编辑和旧历史修复的兼容能力；这部分完整可用的浮窗替代及旧链接转接需要继续处理，不能现在直接删掉业务页。工作台原卡片已取代旧日常试跑方式。
- 本次调试模式**未执行**类型检查、测试、构建、CI 手动触发或 FnOS/Android 真机验收。进度详见 [P15计划](plan/plan-15-legacy-consolidation.md)、[T46任务卡](task/task-46-obsolete-ui-removal.md)、[测试记录](task/test-report.md)。
## 2026-10-28 · T46 第二批：正式脚本和完整条件编辑迁入原手机浮窗
- 手机卡片脚本浮窗已接入正式脚本创建/更新/修订号 CAS、可拖动删除确认和未知写入回执只读核对；Rust 新增独立 `safe-delete` 修订 CAS 路由，旧 DELETE 仅保留兼容，ScriptLibrary 同步改用安全接口。
- 执行历史失败步骤不再进入 ScriptLibrary 复制一套回修工作区，而是统一使用 `onRepairCard` 返回原设备，验证 runId/stepId/设备授权及历史快照；未执行自动重放。
- 卡片时间线支持无手机动作的新增步骤、条件节点转换、Then/Else 分支插入、同分支上移/下移及唯一末尾业务断言勾选。原 Inspector、卡片视频与草稿均维持单一来源。
- 旧 ScriptLibrary 兼容管理页仍保留，不能在未通过编译和 FnOS/Android/双浏览器联调前删除；详见 [T46任务卡](task/task-46-obsolete-ui-removal.md) 与 [测试记录](task/test-report.md)。
## 2026-10-28 · T47 第一批导航兼容与冗余样式清理
旧脚本管理页已增加安全返回原工作台手机脚本浮窗的入口，不再诱导进入第二个试跑媒体流程，并清理确认无模板对应关系的失效样式；其余安全共享 helper 与旧链接保留至真实替代验收。详见 [T47](task/task-47-dead-code-cleanup.md)、[测试记录](task/test-report.md)。

## 2026-10-28 · T47 第二批冗余清理
已进一步清理共享批量入口无用导入、旧脚本页无用 prop 和重复手机目标选择；旧脚本 Tab / 控件定位 / 历史回修共用恢复原卡片可见逻辑，修复被筛选手机卡片原本拿不到引用的问题。继续保留有 T17 安全测试引用的旧移交模块与动态生成的悬浮 CSS。未执行测试/构建/CI 或真机验收，详见 [T47任务卡](task/task-47-dead-code-cleanup.md)。

## 2026-10-28 · T48 第一批可运行但尚未执行的回归断言
已编写并接入 `test:unit` 的批量幂等凭据/单步意图安全断言，同时加固批量凭据 UUID、唯一设备和时间戳的落盘校验。按当前调试模式没有执行 Node、类型检查、CI 或真机测试；详见 [T48任务卡](task/task-48-frontend-regression.md)。

## 2026-10-28 · T48/T49 浮窗身份、正式回执与执行器互斥回归代码

新增前端浮窗堆叠、唯一原手机身份/媒体权限、正式脚本 CAS 未知回执纯逻辑断言；将正式写操作的回执管理从巨型 Vue 工作区抽出并加固损坏凭据 fail-closed 和元数据白名单存储。Executor 新增真实 `Runtime + SQLite Journal` 的只读观看与脚本写锁模拟媒体用例。均为**已提交、未执行**，并不代表全屏、WebRTC、ADB、双浏览器或 FNOS 验收通过。详见 [T48](task/task-48-frontend-regression.md)、[T49](task/task-49-backend-regression.md) 与 [测试记录](task/test-report.md)。
## 2026-10-28 · T49 Rust / Executor 数据库安全回归源码成型

已增加 Rust `automation/tests.rs`（注册于 mod.rs），基于真实迁移链与正式资产 API 的纯内存数据库，覆盖正式脚本创建、CAS 修订/删除、跨用户授权、零步骤不可运行、请求 UUID 幂等和原视频与脚本写锁二阶段移交；加上 Executor 已编写的六条观看/写锁清理异常场景。完整用例见 [T49 测试矩阵](task/task-49-test-matrix.md)。**均尚未执行测试/构建、未做 FnOS 或 Android 真机验收**。下阶段仍需真实执行通过后进入 T50/T51。

## 2026-10-28 · T50 真机验收准备

已将 FINAL V3 18 条 FF 与原 12 条 AC 建立独立的 [FnOS/Android/双浏览器取证矩阵](task/task-50-acceptance-matrix.md)，定义 SHA、digest、session/run/step/request ID 和录像/失败复测字段。该任务现在**仅完成前置文档**，未开展自动测试、部署、真机操作或双浏览器实测；后续须获明确测试授权并依据实测结果更新。
## 2026-10-28 · 用户授权的非真机收尾（T46～T49、T51部分）

- T46：`ScriptLibrary.vue` 旧多功能编辑器/旧写入和试跑入口正式退役，只读脚本目录 + 历史未知请求核验保留，原 PhoneCard 浮窗成为唯一业务操作入口。
- T47：清理弃用事件/Inspector 旧来源与多余参数；不删仍有真正引用的安全 helper、ScriptStepEditor、UiInspectorDialog。防退回测试已接 `test:unit`。
- T48：生产主/子浮窗 Chromium 全屏/ESC/脏编辑/键盘拖动测试接入 GHCR 关口；Node CAS/批量防重放与白名单测试已就位，CI 待最终日志。
- T49：Rust 12 项与 Executor 6 项模拟 DB/媒体安全回归；根据失败 GHCR 日志修复 Rust Storage trait 编译阻塞及前端 TS1005，等待新 Actions 的正式结果。
- T51：更新 [非真机交付追踪](task/acceptance-report.md)；最终签收仍受 T50 30 项 FnOS/两安卓 OEM/双浏览器真机验收阻塞。本轮**不执行任何真实手机控制或 FnOS 部署**。
