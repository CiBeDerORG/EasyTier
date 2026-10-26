# 自动化中心 · 控件—脚本—执行闭环计划

分支：`feat/automation-center`。本文件记录 2026-10-26 需求/开源对标后形成的正式任务，以及代码落实范围。
执行规则：逐项提交；不把 GitHub Actions 通过当成 Android/FnOS 真机验收；与其他协作者同分支开发时先读取最新提交。

| ID | 优先级 | 当前状态 | 范围和验收条件 |
| --- | --- | --- | --- |
| AUT-00 | P0 | **验收规范已提交，等待真机** | `automation-aut00-acceptance.md` A00-01～10；Android 17、其他 Android 版本、FnOS 镜像 |
| AUT-01 | P0 | **已提交第一批，CI 待核对** | `automation-flow-tree.ts` 编辑器树结构与扁平执行协议互转；整块删除；自动计数；禁止默认 HOME 占位；禁止手改分支范围；回归测试 |
| AUT-03 | P0 | **已提交第一批，CI 待核对** | `automation-draft.ts` 单一账号会话草稿，拾取/录制/编辑共用；切换 tab 保留；切脚本确认丢弃；不会把运行参数存入浏览器。**刷新页面后持久恢复尚未实现** |
| AUT-02 | P0 | **第一版已提交，CI/真机待验收** | `automation-gesture.ts` 按时长、最大位移分类短按/长按；明显位移必须手动选「滑动/拖拽/不录制」；超时/回到原点/坏几何拒绝生成；前端回归测试 |
| AUT-04 | P0 | **前端构建与 67 项单元测试通过，主 CI/真机待验收** | 编辑器选中步骤后选择手机并打开无需投屏的控件树，采用原拾取器唯一性判断；仅回填定位条件，保持步骤 ID、原动作、时长、输入参数、条件分支与滚动容器不变；账号/脚本修订/设备绑定变化后拒绝旧结果 |
| AUT-05 | P0 | **前端构建和 75 项单测通过，原生 CI/真机待验收** | 每次打开运行确认弹窗时查询当前账号最新设备绑定与占用状态；3 路并发独立检查 ADB 与需要时的 UiAutomator2；仅就绪设备可勾选；拒绝不完整预检或空设备集；Rust/执行器最终仍二次验证 |
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
- `easytier-web/frontend/src/modules/automation-step-target.ts`：只更新已存在步骤的定位条件，不追加动作，不更改类型、UUID、长按时长、输入参数、分支范围、滚动容器等；拒绝输入与属性断言使用 XPath、非法坐标和伪造字段。
- `easytier-web/frontend/src/components/automation/ScriptLibrary.vue`：脚本编辑器内选择手机并请求对当前步骤重新拾取。
- `easytier-web/frontend/src/components/automation/UiInspectorDialog.vue`：无视频时仍可只读查看手机控件树；编辑已有步骤时不显示新增动作入口，必须命中唯一定位。
- `easytier-web/frontend/src/components/AutomationCenter.vue`：校验账号、脚本 ID/修订、原动作类型与设备 target fingerprint，防止跨账号、跨脚本或设备解绑后误覆盖。
- `easytier-web/frontend/tests/automation-step-target.test.mjs`：现有步骤 UUID、分支计数、输入参数、滚动容器、时长不变以及不合法输入失败关闭的回归。
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
9. 编辑器选择包含 XPath/Resource ID 的步骤，切换不同已授权手机重新拾取控件；若 UiAutomator2 未准备，应显示有明确错误且不会修改原步骤。
10. 对输入、If/Else、滚动查找和长按分别重新拾取；只更新目标，保留参数、分支范围和动作配置。遇到非唯一或已截断 UI 树不得保存。
11. 拾取后若设备指纹、脚本修订或账号发生变化，旧拾取响应必须被拒绝。

## AUT-05 设备预检实施与验收

- `easytier-web/frontend/src/modules/automation-preflight.ts`：根据脚本真实依赖区分原生 ADB 操作与 UiAutomator2 操作，限并发逐台只读状态探测；断线、组件未准备、连接占用、目标设备漂移不能自动入选。
- `easytier-web/frontend/src/components/automation/ScriptLibrary.vue`：运行弹窗内并行预检与逐台状态、重新检查和只选就绪设备；试跑更换目标手机后自动重新检查；参数与运行请求继续保持原有保护。
- `easytier-web/frontend/tests/automation-preflight.test.mjs`：覆盖 ADB-only 脚本不要求组件、设备缺失/离线/未授权/占用/绑定异常、组件状态、3 路并发、单机失败不影响其他设备、关闭后异步结果不得提交。
- `easytier-web/src/db/automation/mod.rs`：设备目录新增只读的 `run_occupied` 状态，使用与既有 `create_run` 相同的运行中/清理未确认状态集合及设备/路由匹配条件；不创建新数据库表。
- **不修改服务端执行准入**：`POST /runs` 的持久任务和设备独占检查、执行器每步授权、未知结果不重发均保留。预检快照不代表资源预留。

验收动作：

1. 选中多台手机，其中一台已占用、一台无线 ADB 已关闭、一台已准备 UiAutomator2。打开批量运行弹窗，三台分别显示原因且互不等待；只允许就绪手机被勾选。
2. 使用仅 `key` 和 `launch_app` 的脚本：在线手机无需 UiAutomator2 就绪即可进入“可执行”。
3. 使用点击和控件断言脚本：UiAutomator2 `not_prepared` 显示“需要准备”，不触发自动安装。
4. 在弹窗打开后修改 ADB 绑定，重新预检时拒绝旧绑定；提交时仍由服务端再次检查。
5. 单机试跑更改目标手机必须重新预检，预检未结束或没有选中就绪设备时禁止提交。
6. 已提交请求的结果未知时，重试仍保持同一个 `request_id`，不可生成新运行记录。
7. 切换账号、关闭弹窗、组件卸载后，旧异步状态不能写回新账号或新弹窗。

## 本轮未改的边界

- 没有接入 Appium Server、Maestro CLI 或 Blockly；仍由 Python UiAutomator2 执行。
- 不允许自由跳转、不引入重试未知动作、不新增 Root/无障碍 APK。
- 草稿目前只在自动化中心页面会话中保留，未写入本地存储或数据库，避免无意保存敏感文本。
- 未完成 Android 17 / FnOS 真实设备测试，不标记 AUT-00 或 AUT-08 完成。

## AUT-04 前端 CI 证据

- 2026-10-26：GH Actions Run [40317280598](https://github.com/CiBeDerORG/EasyTier/actions/runs/40317280598) 的 `Build Web and config generator` job 已成功，`test:unit` 结果 **67 passed, 0 failed**（对应提交 `0370b01`，文档提交不改变可执行代码）。
- 该时刻原生回归仍在运行；不声称整个 GHCR 流水线或 Android 17 真机已通过。

## AUT-05 首轮 CI 证据

- GH Actions [40326165945](https://github.com/CiBeDerORG/EasyTier/actions/runs/40326165945)：Web build job 成功，包含 `automation-preflight.test.mjs` 的前端单元测试 **75 passed, 0 failed**。
- 该记录先于 `run_occupied` Rust 扩展；完整新提交的 native 编译和正式镜像流水线需要后续检查，不能借用前一次通过的 CI 声称全部验收。
