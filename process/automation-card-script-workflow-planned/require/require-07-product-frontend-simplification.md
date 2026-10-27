# FINAL V3 · 产品经理与前端架构师代码取舍报告

**日期**：2026-10-27；**分支**：`feat/automation-center`；**状态**：评审完成、R0 局部前端优化已提交，R1～R5 尚未实施。  
本报告服从 `require-05-final-card-script-workflow.md`，最高原则是**所有自动化交互基于自动化中心手机卡片的唯一原实时视频**。先提供等价替代，再删除旧代码，禁止先删业务能力。

## 一、业务路径审查与取舍

| 用户需求 | 现在的多个入口或重复代码 | 最终决定 | 删除条件 |
|---|---|---|---|
| 当前脚本管理 | 卡片 footer 步骤列表 + 独立 ScriptLibrary 的脚本列表/编辑器 | 共享 owner 级唯一草稿与 Script API；在**原视频浮窗**新建、选中、克隆、删除步骤 | 浮窗完整接线与草稿 CAS 后移除旧卡片 footer 常驻步骤面板 |
| 控件拾取 | PhoneCard 原画面拾取 + UiInspectorDialog 里额外 `<video>` | 只保留主 video，Inspector 的树/属性/动作配置改浮动子窗并高亮主视频 | 主视频拾取/高亮/拖拽/坐标/全屏实测可用后才删第二 video |
| 步骤编辑 | CardStepEditor 内嵌编辑与 ScriptLibrary 页面 ScriptStepEditor | **保留一套 ScriptStepEditor 表单和定位候选算法**，UI 外壳统一浮动子编辑窗 | 浮窗兼容已有参数编辑、重新拾取、演示、保存/冲突/未保存守卫 |
| 单步试跑 | CardTrialPanel 准备与确认 modal + ScriptLibrary 单步运行对话框 | 卡片浮窗一步 ▶，后台按需准备，行内进度/失败修复；去重复风险卡 | viewer/action lease 与回执幂等、安全校验/结果未知机制联调完成 |
| 整套执行 | ScriptLibrary 运行弹窗又开 previewPlayer，并会因准备主动关闭原卡片会话 | 全局脚本 ▶ 只向勾选设备执行；每台设备只看**其卡片视频** | 业务流程与全局 UI 替换成功、Rust/Executor 原子写锁移交后清除独立媒体预览 |
| 编辑/修复跳转 | UiInspector 的“去编辑脚本”、完成录制后 `changeTab('scripts')`、历史跳脚本库 | 打开当前原视频对应的自动化浮窗/子窗；历史回修导航原卡片 | 失败结果回正确 stepId、未保存草稿保护、历史不可变通过验收 |
| 状态与安全 | 卡片、脚本库多处分别展示已发布/已同步、单步/整套的不同回执 | 共享一个 draft sync 状态/一套运行 receipt/结果投影；各处可展示但不能双份写 | 仍区分正式发布和服务端草稿修订，不迁移敏感参数到浏览器存储 |

## 二、绝对保留的基础能力

- `PhoneCard.vue` 的主 `video`、WebRTC 会话/像素映射、手控触点/旋转/全屏基础。
- `automation-draft-sync.ts` + 服务端 CAS / revision / owner 授权及脚本保存；不因脚本换位置而复制数据。
- `automation-locator-candidates.ts` / `automation-pick-engine.ts` / `automation-locator-demo.ts` 的只读控件树与唯一匹配。
- `automation-device-compatibility.ts` 的硬失败、运行 UUID 幂等、`automation-trial-receipt` / RunHistory / 历史快照。
- EasyTier 自研的 `Journal.claim()` 写操作独占与 cleanup_pending/outcome_unknown；未来应拆分**只读观看与写控制权限**，不能简单删锁。

## 三、R0：本轮可安全去除的重复前端事件

**实际发现**：`CardStepTimeline.openEditor()` 每次点击步骤都发送 `select` 和 `edit`；而 `AutomationCenter.editCardStep()` 内部已经调用 `selectCardStep()`，选中同一步被重复处理。

**本轮代码动作**：移除 `CardStepTimeline` 的 `select` emit 和 `openEditor` 双发事件函数；步骤正文与 ✎ 按钮只发送 `edit(stepId)`。`PhoneCard` 不再转发 `step-select`，`AutomationCenter` 不再订阅该冗余事件。**不改上层 editCardStep 的草稿归属校验、未保存参数确认以及最终 stepId 选择。**

**预期体验改善**：一次点击只处理一次编辑；避免重复选中造成中间态变化，并为未来主浮窗统一事件合同提供单入口。

**未改**：媒体、Inspector、CardTrialPanel、ScriptLibrary、运行权/独占锁、脚本 API；移除它们现在会破坏已有闭环。

## 四、后续分批代码瘦身（必须先替换才能删除）

1. **R1（FINAL-09）**：浮动 icon 取代原唤醒槽、唤醒上移；主/子浮窗容器、拖动边界与 Fullscreen top layer 接入；不重复 video。
2. **R2（FINAL-02/03）**：脚本下拉首行新建、点击切换、克隆改名后新 Script 记录、每步 🗑（含条件分支）、CAS；时间线和步骤编辑迁到浮窗后才能删除卡片 footer 常驻区。
3. **R3（FINAL-07）**：UiInspector 控件树/属性与定位候选迁入浮窗并高亮唯一主画面；实测后删 Inspector 第二播放器及“去脚本库编辑”的旧跳转。
4. **R4（FINAL-04/05/06）**：视频 viewer/action 写锁分权；浮窗绿色 ▶ 直接本机试跑、全局 ▶ 执行已选机；真实安全交接完成后淘汰旧运行 modal 与 ScriptLibrary 预览视频，**保留其请求状态机与后端 API**。
5. **R5（FINAL-08/10）**：删除经过替代的无引用组件、样式、props/events/import 与旧回归测试重复项；FnOS + 双 OEM + 双浏览器/全屏/结果未知实测后关单。

**验收门槛**：不能为了代码少而删掉未替代的创建/编辑/重拾/修复、历史、双向排他或多浏览器冲突保护。每个阶段独立评估并报告“已改代码、已跑自动测试、已真机验收”三种不同状态。

## 五、调试模式事实

仅 R0 前端事件去重与本文档已修改；**没有执行**本地测试、TypeScript 编译、lint、构建、手动 CI、FnOS 或 Android 真机验证。R0 后续需重点验证点击行文本与编辑图标、从 A 步改 B 步时取消放弃未保存修改、跨两台手机卡片编辑身份是否保持。
