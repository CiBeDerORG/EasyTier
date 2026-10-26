# 自动化中心 · 控件—脚本—执行闭环计划

分支：`feat/automation-center`。本文件记录 2026-10-26 需求/开源对标后形成的正式任务，以及代码落实范围。
执行规则：逐项提交；不把 GitHub Actions 通过当成 Android/FnOS 真机验收；与其他协作者同分支开发时先读取最新提交。

| ID | 优先级 | 当前状态 | 范围和验收条件 |
| --- | --- | --- | --- |
| AUT-00 | P0 | **验收规范已提交，等待真机** | `automation-aut00-acceptance.md` A00-01～10；Android 17、其他 Android 版本、FnOS 镜像 |
| AUT-01 | P0 | **已提交第一批，CI 待核对** | `automation-flow-tree.ts` 编辑器树结构与扁平执行协议互转；整块删除；自动计数；禁止默认 HOME 占位；禁止手改分支范围；回归测试 |
| AUT-03 | P0 | **已提交第一批，CI 待核对** | `automation-draft.ts` 单一账号会话草稿，拾取/录制/编辑共用；切换 tab 保留；切脚本确认丢弃；不会把运行参数存入浏览器。**刷新页面后持久恢复尚未实现** |
| AUT-02 | P0 | **第一版已提交，CI/真机待验收** | `automation-gesture.ts` 按时长、最大位移分类短按/长按；明显位移必须手动选「滑动/拖拽/不录制」；超时/回到原点/坏几何拒绝生成；前端回归测试 |
| AUT-04 | P0 | 未开始 | 既有步骤重新打开拾取器；唯一性验证；步骤 ID 不变 |
| AUT-05 | P0 | 未开始 | 异步逐台 ADB/控件/独占预检；最终派发仍在服务端校验 |
| AUT-06 | P0 | 未开始 | 区分「脚本完成」「业务验证成功」「业务未验证」；旧数据不误报 |
| AUT-07 | P0 | 未开始 | 失败步骤与脚本修订、设备、证据关联；跳转当前编辑器 |
| AUT-08 | P0 | 未开始 | FnOS + 多台 Android 端到端验收与发布 |
| AUT-09 | P1 | 未开始 | 安全单步诊断，保持独占、授权及未知结果不重放 |
| AUT-10 | P1 | 未开始 | 候选控件半自动匹配，用户确认，禁止自动退化到不稳定序号定位 |

## 已提交的第一批实现

- `easytier-web/frontend/src/modules/automation-flow-tree.ts`：树解析、编译、分支插入、条件转换及整块删除。
- `easytier-web/frontend/src/components/automation/ScriptLibrary.vue`：树操作接入、未配置占位、共享草稿及覆盖确认。
- `easytier-web/frontend/src/components/automation/ScriptStepEditor.vue`：锁定条件类型/跨度；补充 If/Else 定位配置。
- `easytier-web/frontend/src/modules/automation-draft.ts`：按账号隔离的页面会话草稿和去重合并。
- `easytier-web/frontend/src/components/AutomationCenter.vue`：录制和拾取直接追加同一草稿。
- `easytier-web/frontend/src/modules/automation-gesture.ts`：O(1) 采样轨迹统计，短按/长按与位移手势分类；无法判断滑动还是拖拽则阻止新增动作并请求用户确认。
- `easytier-web/frontend/tests/automation-gesture.test.mjs`：长按 500/800/1500/3000ms、位移确认、返回原点、异常几何和超时的回归测试。
- `easytier-web/frontend/tests/automation-flow-tree.test.mjs`、`automation-draft.test.mjs`：结构与草稿回归用例。
- `.github/workflows/node-attributes-image.yml`：自动化分支主镜像构建前新增前端 `test:unit`。

## 回归要点

1. 旧脚本含三级 If/Else 嵌套：可加载、保存，原步骤 ID 和执行协议不变。
2. 删除嵌套条件时，不再遗留原分支动作；删除唯一分支子步骤时拒绝而非默默重划范围。
3. 添加空白步骤后不能直接保存/试跑；必须显式选择按键或操作类型。
4. 在设备画面拾取 A、录制 B，进入脚本库应保留 A+B；返回设备工作台再进脚本库，草稿不丢失。
5. 已编辑未保存的脚本切换到另一已保存脚本必须先确认。
6. 切换托管账号后旧账号草稿不能继承到新账号。
7. 录制和拾取不主动控制正在运行脚本的设备；原有设备独占与授权不变。
8. 通过前端 CI 后仍需人工手机试跑，并补齐 AUT-00 的环境矩阵。

## 本轮未改的边界

- 没有接入 Appium Server、Maestro CLI 或 Blockly；仍由 Python UiAutomator2 执行。
- 不允许自由跳转、不引入重试未知动作、不新增 Root/无障碍 APK。
- 草稿目前只在自动化中心页面会话中保留，未写入本地存储或数据库，避免无意保存敏感文本。
- 未完成 Android 17 / FnOS 真实设备测试，不标记 AUT-00 或 AUT-08 完成。
