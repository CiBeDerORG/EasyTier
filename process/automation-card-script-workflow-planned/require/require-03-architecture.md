# 技术经理实现合同与实现难度评估

## 现有可复用源代码（以该分支审查为准）
- `PhoneCard.vue`：WebRTC原视频、真实内容区域坐标映射、控制输入；现有 `@contextmenu.prevent` 无业务菜单，拾取图标并非开关。
- `UiInspectorDialog.vue`：UiAutomator树、候选匹配、唯一性校验、坐标兜底、添加/替换；不得复制实现。
- `AutomationCenter.vue`：账号范围、单份 `draftWorkspace`、拾取事件协调、浏览器本页会话。
- `automation-draft.ts`：追加并选中步骤；**目前只在会话内存，不持久化**。
- `ScriptLibrary.vue`：脚本保存/修订/分支/单步试跑；`RunHistory.vue`：记录、诊断及修复请求。
- `automation-preflight.ts`、后端 `create_run()`：预检及运行设备排他；新方案应复用而非绕过。
- `automation-locator-candidates.ts`：唯一候选，不允许忽略匹配不唯一而偷偷保存。

## 架构合同
1. PickerContext = `{ownerId,deviceId,targetFingerprint,mode:'off'|'preparing'|'on'|'error',intent:'append'|{replaceStepId},generation}`。账号/设备/会话变更时失效；拾取模式不等于录制。
2. 视频右键由卡片负责获得实际图像像素坐标（考虑 letterbox、resize、旋转、全屏）；弹出菜单前禁止冒泡/禁止向 scrcpy 发送事件。定位与动作配置由共享 Picker/Inspector composable 负责。
3. 获取树走已有授权 `GET /devices/{id}/hierarchy`，树+设备指纹+屏幕尺寸+采集时间组成版本；标记过期时必须刷新；服务端验证已存在的权限不能下放为「只在前端检查」。
4. 草稿单一真实源，卡片时间线和脚本库订阅同一份；更新目标与新增步骤用两个不同命令，替换时旧 ID、动作与参数稳定；禁止在卡片复制一套脚本逻辑。
5. 「演示」只读查找并在本地画面叠加矩形框；可先使用现有 hierarchy API，若性能/服务端能力不足再评估新只读 match API，绝不走 executor tap。
6. 草稿持久化需**独立服务端草稿存储及版本化 API**，不直接将每次拾取写为已保存正式脚本；服务端按 owner 授权、CAS revision 和唯一 draft ID 防多标签页覆盖。只保存脚本结构及参数名；不保存密码/运行时参数值到浏览器存储或草稿。
7. 执行器依旧独占动作控制；试跑前释放**本页明确持有**的预览，确认清理后再交给 runner，保留后端 BEGIN IMMEDIATE 二次拦截。不能为了演示穿透独占控制。
8. 状态事件 `draftId/stepId/deviceId/runId` 需有明确关联；查询超时/结果未知时不可自动重复动作。
9. 全屏属性面板必须渲染到正确 top-layer；触屏长按仅在拾取开关开启时才触发，不干扰正常手机长按。

## 复杂度与风险（技术经理）
- 低：拾取开关状态、追加后关窗、列表展示、删除与选择。
- 中：右键菜单、画面坐标转换、组件复用、步骤重新拾取、只读高亮演示。
- 中高：服务端草稿及修订冲突、恢复与多页面编辑、回步数据关联。
- 高：执行时预览所有权移交、后端排他防竞态、异步设备准备/执行状态联调。
- 关键原则：**不要求新建执行协议才能做 M1/M2**；M4 先复用现有 prepare/preflight/runs/repair，确认缺口后再做最小后端改动。
