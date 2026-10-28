# T45 · 完整流程只观看各手机原卡片视频

> 状态：**每台原卡片Viewer及断流展示已接线，未实机验收**｜计划：P14｜复杂度：M｜目标分支：`feat/automation-center`｜产品合同：`../require/require-05-final-card-script-workflow.md`

## 唯一负责人/依赖
- Owner：**卡片 Agent + 集成负责人串行接线**。
- 文件边界：PhoneCard.vue、AutomationCenter.vue；淘汰 preview Player 另归 T46。
- 开始条件：T39、T43、T44。

## 实现步骤
1. 批量运行时 A 使用 A 卡片现有 session/MediaStream，B 使用 B 卡片已有视频；不按后台任务另建 video tag。
2. 设备没有主动打开预览时在原卡片建立只读观看，不借 ScriptLibrary 在弹窗中另起播放器。
3. 全屏进入 A 卡片继续看 A 的步骤状态，不因 B 的执行停止 A 视频；画面本身断线真实提示。

## 通过标准
- [ ] 同设备自动准备/执行没有应用主动 close() 原视频，视频断流明确报错。
- [ ] 设备 B 完成/失败不抢 A 的视频会话或浮窗控件。
- [ ] 交接明确修改文件、调用契约、已知风险与证据，不把代码提交当真机验收。

## 风险及禁止项
严禁以第二条低帧率 WebRTC 替换“唯一实时画面”；视频背景装饰不视为第二可操作视频。

## 本任务当前状态
已提交代码，仍未执行检查、测试、手动 CI 或真机操作。后续执行须遵守 `AGENTS.md` 调试模式。

## 2026-10-28 · T45 代码交付
- `useFloatingBatchRun` 每台先使用其 `PhoneCard.getSessionId/getStream`；没有观看会话时只在对应原卡片执行 `open`，禁止接管其它网页的占用会话。随后通过 T39 的 `enterViewOnly()` 释放手控写权限，保持此 `sessionId` 和原 WebRTC `MediaStream`，脚本准备/执行不调用 `close()`。
- `PhoneCard` 的原视频容器显示本机批量状态；`FloatingBatchRunPanel` 按各设备展示自己的 sessionId 前缀和独立 Viewer 状态，stream 中断/身份变更按真实媒体异常提示，不假称“还在看”也不误取消其它设备脚本。
- 运行期间 `AutomationCenter` 阻止自动化切 Tab、账号、筛选导致当前原视频被关闭，并阻止关闭选中设备原画面；服务端真实动作写权限仍由 Rust/Executor 独占锁控制，UI 禁用不能代替后端。
- 旧 `ScriptLibrary` 仍存在历史 previewPlayer 和关闭旧预览的兼容流程，**新批量入口不使用它们**。彻底删旧独立播放器及重复业务状态归 T46“先替代后删除”，本阶段不冒险直接删掉其安全验证逻辑。
- **状态：提交不等于实测。待 FnOS、多个 Android、旋转/全屏、两浏览器、弱网和同时停止回归；未主动运行任何检查/CI/构建。**
