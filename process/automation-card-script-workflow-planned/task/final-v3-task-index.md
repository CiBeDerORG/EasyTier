# FINAL V3 · 开发任务可执行索引 T19～T51

> 最新产品合同：`../require/require-05-final-card-script-workflow.md`；代码取舍：`../require/require-07-product-frontend-simplification.md`；按需求计划：`../plan/README.md`。
> **创建任务文件不代表实现。** 本页 33 个新任务均为“待开发/待执行”；旧 T01～T18、已有 R0 前端去重代码与历史测试状态不作废。

## 单任务列表

| 任务 | 内容 | 计划 | 负责人 | 波次 | 主要依赖 | 状态 |
|---|---|---|---|---|---|---|
| [T19](task-19-baseline-audit.md) | 基线审计 | P07 | 架构 | W0 | 无 | 审计已完成（未测试） |
| [T20](task-20-contract-and-events.md) | 状态事件契约 | P07 | 集成 | W0 | T19 | 契约已提交，待测试 |
| [T21](task-21-floating-host.md) | 主浮窗 | P08 | 卡片 | W1 | T20 | 浮窗壳已提交；待 T22 入口/测试 |
| [T22](task-22-orb-layout.md) | icon 重排 | P08 | 卡片 | W1 | T21 | 代码已提交，待验证 |
| [T23](task-23-fullscreen-portal.md) | 全屏宿主 | P08 | 卡片 | W1 | T21 | 代码已提交，待验证 |
| [T24](task-24-child-floating-window.md) | 子浮窗 | P08 | 浮窗 | W2 | T23 | 公共组件已提交，待接线/验证 |
| [T25](task-25-empty-script-backend.md) | 0 步脚本服务端 | P09 | Rust-脚本 | W1 | T20 | 代码已提交，待验证 |
| [T26](task-26-script-selector.md) | 脚本窄下拉/新建 | P09 | 脚本/集成 | W2 | T21,T25 | 代码已提交，待验证 |
| [T27](task-27-script-clone.md) | 克隆改名确认 | P09 | 脚本 | W2 | T24,T26 | 代码已提交，待验证 |
| [T28](task-28-draft-switch-cas.md) | 草稿 CAS 防丢 | P09 | 集成 | W2 | T20,T26,T27 | 代码已提交，待验证 |
| [T29](task-29-timeline-migration.md) | 步骤列表迁入浮窗 | P10 | 步骤/卡片 | W3 | T21,T26 | 代码已提交，待验证 |
| [T30](task-30-ordinary-step-delete.md) | 普通步骤删除 | P10 | 步骤 | W3 | T28,T29 | 代码已提交，待验证 |
| [T31](task-31-branch-delete-undo.md) | 条件分支删除/撤销 | P10 | 步骤 | W3 | T24,T29,T30 | 代码已提交，待验证 |
| [T32](task-32-floating-step-editor.md) | 步骤编辑浮窗 | P10 | 步骤 | W3 | T20,T24,T29 | 代码已提交，待验证 |
| [T33](task-33-inspector-main-video.md) | 原视频属性树 | P11 | Inspector | W2 | T20,T23 | 代码已提交，待验证 |
| [T34](task-34-inspector-gesture-fullscreen.md) | 控件树/拖拽全屏 | P11 | Inspector | W3 | T23,T33 | 代码已提交，待验证 |
| [T35](task-35-inspector-legacy-exit.md) | 去 Inspector 重复视频 | P11 | Inspector/集成 | W4 | T32,T33,T34 | 代码已提交，待验证 |
| [T36](task-36-lease-contract.md) | Viewer/Action 合同 | P12 | 架构 | W1 | T20 | 合同已提交，待验收 |
| [T37](task-37-executor-lease.md) | Executor 动作租约 | P12 | Executor | W2 | T36 | 代码已提交，待联调 |
| [T38](task-38-rust-lease.md) | Rust 观看/动作权限 | P12 | Rust-运行 | W2 | T36,T37 接口 | 代码已提交，待联调 |
| [T39](task-39-frontend-readonly-lease.md) | 前端只读运行态 | P12 | 卡片/集成 | W3 | T23,T37,T38 | 单步/卡片接线，批量待迁移 |
| [T40](task-40-single-step-orchestrator.md) | 单步 ▶ 自动执行 | P13 | 运行 | W4 | T32,T34,T39 | 代码已提交，待浏览器与真机验证 |
| [T41](task-41-single-step-progress.md) | 单步行内进度 | P13 | 运行 | W4 | T40 | 行内进度接线，待完整验收 |
| [T42](task-42-single-step-repair.md) | 单步修复与旧模态退役 | P13 | 修复/集成 | W4 | T40,T41 | 回修浮窗已接线，待闭环验收 |
| [T43](task-43-batch-submission.md) | 全流程启动 | P14 | 运行/集成 | W4 | T28,T39 | 主链路已提交，待联调 |
| [T44](task-44-batch-progress.md) | 并行进度/停止 | P14 | 运行/历史 | W4 | T41,T43 | 并行进度/单机与全停已接线，待验证 |
| [T45](task-45-batch-original-video.md) | 各机原视频观察 | P14 | 卡片/集成 | W4 | T39,T43,T44 | 每机原画面观察代码已提交，待实测 |
| [T46](task-46-obsolete-ui-removal.md) | 替代后移除旧 UI | P15 | 集成 | W5 | T29,T32,T35,T42,T45 | 主浮窗正式CRUD/条件编辑/历史回修已接线，旧页待验收后退役 |
| [T47](task-47-dead-code-cleanup.md) | 冗余事件样式整理 | P15 | 前端架构 | W5 | T46 | 旧链接转主浮窗/死CSS首批提交，待联调 |
| [T48](task-48-frontend-regression.md) | 前端自动回归 | P16 | QA-Web | W3+ | 相关 UI 功能 | 待开发 |
| [T49](task-49-backend-regression.md) | 后端租约/空脚本回归 | P16 | QA-后端 | W3+ | T25,T37,T38 | 待开发 |
| [T50](task-50-real-device-acceptance.md) | FnOS/安卓/全屏真机 | P16 | QA-真机 | W5 | T48,T49+所有业务 | 待开发 |
| [T51](task-51-final-acceptance-report.md) | 总体验收追踪与交付 | P16 | QA/产品 | W5 | T50 | 待开发 |

## 领取任务与多 Agent 唯一写入表

| 文件/模块 | 唯一写入者与串行规则 | 交接方向 |
|---|---|---|
| `AutomationCenter.vue`、`ScriptLibrary.vue`、shared draft/API | **集成负责人独占** | 脚本/步骤/运行 Agent 交付新组件和 events，由集成负责人统一接线 |
| `PhoneCard.vue`、悬浮 orb、主 video/全屏表面 | **卡片 Agent 独占** | 集成负责人提供 props/emit 合同，浮窗组件以 slot/事件消费，不直接改 PhoneCard |
| `AutomationFloatingPanel` / `AutomationFloatingDialog` / 定位 composable | **浮窗 Agent**，T21/T23/T24 涉及共享 helper 串行 | 向脚本/步骤/Inspector/运行 Agent 提供统一 host/container |
| `CardStepTimeline.vue`、`CardStepEditor.vue`、步骤删除纯函数 | **步骤 Agent 独占** | 集成负责人接共享草稿 CAS 与 script selector |
| `UiInspectorDialog.vue`、`InspectorPropertiesPanel.vue`、候选纯函数 | **Inspector Agent 独占** | 卡片 Agent 对接原 video 的 geometry / 高亮事件 |
| `CardTrialPanel.vue`、运行 status composable | **运行 Agent**，删除旧 trial modal 需由集成负责人安排串行 | Rust/Executor 提供 lease API；T42 后才淘汰旧 UI |
| `RunHistory.vue`、失败结果与修复 | **修复/历史 Agent 独占** | 执行 Agent 提交 run 状态 ID 与不可变快照 |
| `automation-executor/automation_executor/{journal,runtime,media}.py` | **Executor Agent 独占** | 与 Rust Agent 先冻结契约，不并发跨写 |
| `easytier-web/src/db/automation/{scripts,validation}.rs` | **Rust 脚本数据 Agent 独占** | T25 空脚本与 run 拒绝双限制 |
| Rust run/session/authorization 相关接口 | **Rust 运行 Agent 独占** | T38 与 Executor T37 协议契约一致后联调 |
| 自动化测试新增文件与 `test-report.md` | QA / 项目集成负责各自不重叠部分；**一份报告仅由 QA 顺序更新** | 不改生产文件，不伪造测试 |

## 推荐启动顺序
1. **T19 → T20**：确定状态合同与旧 UI 替换边界，再动共享集成文件。
2. **T21～T24**：先让“自动化”图标、主/子浮窗与全屏连续可用，保持业务容器和运行逻辑解耦。
3. **T25～T28** 与 **T36～T38** 可在合同冻结后不同 Agent 并行：一条负责脚本生命周期，一条负责独占安全；`AutomationCenter.vue` 的接线仍需安排串行。
4. **T29～T35**：脚本与步骤迁到浮窗、Inspector 只使用原主画面；不得在替代未通前删旧 UI。
5. **T39～T45**：观看/动作权限联调完成后，做单步 ▶ 和完整流程 ▶ 的同画面真实执行、反馈与失败修复。
6. **T46～T51**：清旧代码、自动回归（待授权）、FnOS + 真机 + 双浏览器验收、报告结项。

## 统一完成条件
- **业务正确性**：主视频唯一且原流不停；卡片单步只操作本设备；完整流程只操作选中设备；脚本/草稿只有共享唯一状态；条件步骤安全删除；克隆必生成独立 ID。
- **安全正确性**：owner/fingerprint、坐标/控件匹配、CAS、run request 幂等、action lease 独占、cleanup_unknown fail closed 都保留。
- **交付正确性**：每一任务有清晰代码差异、接口/文档、回归风险和真实验证证据。调试模式不擅自跑本地测试/构建/手动 CI。


> 2026-10-28：T36～T39 提交记录和未验收清单见各任务卡及 `test-report.md`。T40 全屏单步、T43 批量和 T45 多机原画面持续观察没有在本阶段标完成。

> 2026-10-28 T40～T41：本机原视频内单步浮窗、一次请求UUID、自动安全检查/准备、单步状态与真实耗时已提交；未经 CI/浏览器/手机验收。T42 及批量任务仍未完成。

> 2026-10-28 T42：失败回修迁到原视频子浮窗、保存修订后人工再试跑，仍需联调与真机验收。

> 2026-10-28 T43～T45：工作台批量执行、逐机真实回执/单机停止、唯一原视频媒体观察及并发值迁移已接线。旧运行库的副播放器仍在 T46 安全删除范围内，全部未实机验收。

> 2026-10-28 T46：已删除 ScriptLibrary 第二操作预览视频、重复提交 Dialog 与 AutomationCenter 旧预览资源编排，并保留旧试跑未知 UUID 只读恢复。正式脚本管理兼容入口未下线，T46 仍待完成其浮窗替代及联调。

> 2026-10-28 T46 第二批：正式脚本 CAS 发布与安全删除、子浮窗、未知回执拦截、历史失败回原手机卡片、条件 Then/Else 插入、同分支排序和唯一末尾业务断言已提交。旧 ScriptLibrary 兼容页仍保留，测试/CI/真机均未执行。

> 2026-10-28 T47：旧脚本管理入口已支持返回原 PhoneCard 脚本浮窗；清理失效 CSS 的第一批提交完成。旧管理页与共享安全 helper 仍在，待有替代验收/引用证据才退役。

> 2026-10-28 T47 第二批：清理批量编排器无引用 import、ScriptLibrary 死 prop/重复目标计算；修复旧脚本/控件定位中被筛选手机无法返回原卡片的问题；统一三个调用点的原卡片恢复函数，持久正式写入身份在工作区初始化即恢复。旧 handoff 保留，因为 T17 安全回归测试仍引用。未执行编译、CI 或真机回归。
