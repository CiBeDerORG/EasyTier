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
| AUT-06 | P0 | **代码与回归测试已提交，CI/真机待验收** | 只允许显式标记最后一个、分支外的 `assert` / `assert_property` 为业务成功验证；标记保存在原脚本步骤及不可变运行快照中，派发 Python 前剥离；Rust 从逐台最终断言结果推导业务状态，旧运行一律未验证 |
| AUT-07 | P0 | **前端构建及 89 项单元测试通过，最新全量 CI/真机待验收** | 失败步骤关联历史执行快照、原步骤 UUID、脚本修订与设备 ADB 目标；支持一键定位编辑。同修订可编辑当前脚本，较新/删除/内联脚本只能从不可变快照创建独立草稿；未知动作不自动重放。截图仅查看历史原有证据 |
| AUT-08 | P0 | **跨模块与执行器模拟回归已通过；真机/正式发布待验收** | `automation-closure-integration.test.mjs` 串联录制→草稿→脚本树→重拾取→异步预检→两设备验证→历史失败修复；Python 真实调度器+SQLite journal 模拟双手机不同分支与最终断言失败；FnOS/Android A17 实测见 `automation-aut08-e2e-acceptance.md` |
| AUT-09 | P1 | **前端构建和 97 项单元测试通过，最新完整 CI/真机待验收** | 选中步骤直接单机试跑，按实际步骤类型区分只读检查与需确认的动作；If/Else 拒绝脱离上下文；复用已有 REST 运行任务、设备预检、独占与鉴权，不新增特权 ADB 接口；标记为业务验证的断言单独试跑时不会冒充完整脚本成功 |
| AUT-10 | P1 | **第一版已提交，前端 CI / FnOS Android 真机待验收** | 候选控件半自动匹配，当前完整快照内精确唯一性校验，人工确认；禁止自动退化到不稳定序号定位 |

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

## AUT-06 业务验证状态闭环

### 操作方式

1. 在脚本库末尾创建 `assert`（检查控件存在）或 `assert_property`（检查控件布尔属性）步骤，并确保它处于所有 If / Else 条件分支以外。
2. 选中该步骤，勾选「将这一步作为业务成功验证」并保存脚本。普通断言不会被隐式认定为业务成功。
3. 单机试跑或运行已保存脚本；执行器继续正常执行原断言，不接收额外的 `business_verification` 元数据。
4. 执行记录分别显示整个任务和每台手机的执行状态与业务验证状态。
5. 如果断言通过且整台设备脚本完整执行，显示「业务已验证」；断言失败显示「业务验证失败」；根本未执行到断言显示「业务条件未到达」；设备存在结果未知时显示「业务结果未知」。
6. 多台手机结果不一致时显示「部分设备业务未验证」；不以一台成功代表整个批量成功。

### 技术契约

- 标记形式：末尾断言步骤的 `"business_verification":true`，由 Rust `validation::steps` 校验。此标记随已有 `steps_json`、版本修订、不可变 `snapshot_json` 保存，**无需数据库迁移**。
- `easytier-web/src/db/automation/runs.rs`：仅根据快照、验证步骤 UUID 和已持久化的设备结果计算 `business_verification`；派发 Python 的步骤副本中剥离标记，保持执行协议不变。
- `easytier-web/frontend/src/modules/automation-business-verification.ts` 和前端 UI：控制显式标记、展示配置状态和逐台历史结果。
- 单元测试：`tests/automation-business-verification.test.mjs` 与 Rust 模块中的 `business_verification_contract_tests`、`business_verification_run_tests`。验证标记只有一个且属于最后一个独立断言，旧记录不会追认为已验证，失败/未知/未到达严格区分。
- 敏感文本：验证标记仅为布尔值，不包含额外业务数据，原有执行参数不落库保护保持不变。

### 真机验收

- 一台手机业务成功，标记断言通过且脚本完整执行 → 业务已验证；另一台断言失败 → 业务验证失败。批量汇总应为部分验证。
- 断言之前 ADB 断线、执行器中断、取消脚本 → 不得显示「业务已验证」。
- 升级前已有断言的历史执行和未标记脚本 → 业务未验证。
- 历史执行重跑需继承原快照中的显式标记，但重新生成独立的运行 ID，不修改旧结果。
- 旧 Python Executor 不需增加动作类型，但需检查新 Rust 版确实剥离纯业务字段。

## AUT-07 失败诊断与编辑修复闭环

### 从运行记录定位失败步骤

1. 自动化中心 → 执行记录 → 选择任务和设备 → 展开步骤。
2. 对明确为 `failed`、`interrupted`、`outcome_unknown` 的步骤展示只读诊断：原运行 ID、步骤 UUID、脚本 ID / 修订、设备名称及 ID、运行时记录的 ADB 地址/端口/绑定指纹、步骤时间、错误文字、所在 If/Else 分支。
3. 原来已有截图证据的步骤仍可点击查看，但**不会为了查看失败结果自动访问设备或重新截图**。受鉴权保护的截图接口不变，不额外持久化任何密码或 UI 树。
4. 点击「定位并修复此步骤」将历史运行快照与原始失败步骤传给脚本编辑器。
5. 编辑器先调用当前账号脚本列表：
   - 同一脚本 ID、相同修订且完整步骤一致 → 加载该保存脚本，选中原步骤；后续保存产生新修订。
   - 原脚本已更新、被删除、修订或步骤不一致，或历史任务本身为内联试跑 → **从历史快照生成独立草稿**，`editingId` 和 `revision` 清空，保存时必须创建新脚本。
   - 读取当前脚本失败 → 保留当前草稿并提示错误，绝不在无法验证修订时直接编辑旧 ID。
6. 若当前工作区有未保存草稿，先确认是否丢弃。取消不会修改现有内容；只读执行历史不被写回。步骤定位仅修改编辑器选中步骤，不提交脚本，也不对手机执行动作。
7. `outcome_unknown` 要先人工核实手机实际结果，再进行任何独立新执行，且仍遵循 AUT-05 预检及服务端独占/授权。

### 实现文件与回归

- `easytier-web/frontend/src/modules/automation-run-repair.ts`：历史失败步骤鉴别、不可变快照复制、相同修订/不同修订分流。
- `easytier-web/frontend/tests/automation-run-repair.test.mjs`：同修订、冲突、删除、内联、未知结果、无关步骤不允许修复、UUID/条件结构保留测试。
- `easytier-web/frontend/src/components/automation/RunHistory.vue`：逐条诊断和跳转按钮，保留原截图证据；`AutomationCenter.vue` 传递一次性修复事件；`ScriptLibrary.vue` 实时校验版本与丢弃草稿确认。
- `easytier-web/src/db/automation/runs.rs`：返回持久化的运行时 ADB 目标 IP/端口/绑定指纹三个明确字段，绝不返回完整 target JSON 或额外密钥；**无数据库迁移**。

### 真机验收

- 试跑脚本的单台设备断言失败，直接进入脚本编辑器：必须定位到历史失败步骤并以新草稿保存。
- 已保存脚本失败但之后有人修改：绝不能修改新版本，必须生成独立修复草稿。
- 历史截图已存在时可以查看；不存在时不显示虚假的证据链接。
- 前面步骤执行失败而后面步骤 `skipped` 时，只能从真正失败的步骤进入修复。
- 多设备中某台失败、另一台成功：诊断入口只指向该台的失败结果。
- 账号切换和取消未保存确认不会泄露其他账号脚本，旧请求不能重放。
- Android 控件重拾取、实际执行、断线未知结果及原生编译通过仍需独立验证。

## AUT-08 端到端模拟回归、真机验收与发布门禁

- 前端新增 `easytier-web/frontend/tests/automation-closure-integration.test.mjs`，复用真实 TypeScript 模块进行跨模块联动测试，包括录制长按、脚本树解析/编译、重选控件、按需检查 UiAutomator2、两台手机不同业务结果及历史修复复制；包括离线手机、占用、未标记断言、未知结果的反例。
- 执行器新增 `automation-executor/tests/test_device_exit.py` 中 `test_aut08_two_phone_success_and_terminal_assert_failure`，使用真实调度器+SQLite journal，手机 RPC 模拟；验证两台不同分支、一台业务断言成功、一台失败，以及失败隔离、不重复发送断言。
- 两条 CI：Web GHCR [40331799359](https://github.com/CiBeDerORG/EasyTier/actions/runs/40331799359) 的 Web build / 前端 91 单测已成功；executor GHCR [40331918668](https://github.com/CiBeDerORG/EasyTier/actions/runs/40331918668) 已完整成功。Web 的完整 native / package / publish 状态仍须以最新 CI 为准。
- 真机验收：`docs/automation-aut08-e2e-acceptance.md` 定义 G1～G6 门禁与 E08-01～E08-16 现场矩阵。由于没有连接用户 FnOS/Android，本轮全部现场用例均**未测**，不得批准正式发布。
- 对外状态准确表达：**模拟端到端闭环完成，FnOS/Android 真机端到端及发布准入未完成**。

## AUT-09 单步骤试跑实现与验收

### 操作与授权边界

1. 在自动化中心「脚本库」选中一个已配置步骤，点击「单独试跑此步骤」。
2. 条件分支控制节点 `if_gone` / `if_else` 必须随完整脚本运行，不能跳过条件的上下文直接试跑。其余操作会先保存**仅含当前步骤**的临时运行快照，保留原步骤 UUID，并从单步快照中去掉整条业务成功验证标记。
3. 弹窗选择**一台**当前账号已授权且绑定有效的手机，然后复用 AUT-05 设备预检：最新设备绑定、手动控制或其他脚本占用、ADB 与必要时的 UiAutomator2 都需要就绪；组件未初始化时只能返回工作台主动准备。
4. `wait_for`、`wait_gone`、`assert`、`assert_property` 作为只读检查，不要求额外副作用确认；`key`、`tap`、`long_press`、`swipe`、`drag`、`input_text`、`launch_app`、`scroll_find` 均可能改变应用状态，必须主动勾选风险确认；`screenshot` 虽不触碰手机应用，但会持久化截图证据，同样需要确认。
5. 直接调用现有 `POST /runs`（`device_ids` 仅 1 项、`concurrency=1`、`steps` 仅 1 项），原 Rust 账号授权、设备指纹与占用检查、Python 每步授权/独占和未知结果不重发的约束保持不变。**没有创建新的高权限直接 ADB 接口**。
6. 断言单步通过仅证明这一检查在测试时刻通过，不等于整套业务流程完成。脚本运行历史会以独立的 `单步骤诊断` 任务登记；自动化中心不会静默重复动作。
7. HTTP 请求结果未知时保留原 `request_id` 与相同请求内容；人工重试此 HTTP 接收操作不会新建任务 ID，但不代表允许重试结果未知的手机动作。必须检查执行历史和手机实际结果之后才能另发新任务。

### 回归与待验收

- `easytier-web/frontend/src/modules/automation-single-step.ts`：风险分级、安全复制、无条件分支/整脚本元数据隔离，独立单步运行请求。
- `easytier-web/frontend/tests/automation-single-step.test.mjs`：只读分类、不同危险动作确认、业务断言标记剥离、敏感输入参数只存在此次运行参数、脚本其他步骤不混入。
- `easytier-web/frontend/src/components/automation/ScriptLibrary.vue`：步骤入口、设备选择、预检、确认框、仅一次请求和普通任务提交后的历史入口。
- FnOS + Android 真机仍需验证：分别试跑断言、点击、长按、输入与截图，确认没有绕过主服务独占、鉴权和执行器；测试不应针对真实支付或不可逆业务执行。
- CI 前端回归与正式镜像测试以最新 GitHub Actions 结果为准；**代码完成不等于 AUT-09 真机验收完成**。

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

## AUT-07 首轮 CI 证据

- GitHub Actions [40330581522](https://github.com/CiBeDerORG/EasyTier/actions/runs/40330581522)：Web build 与 test:unit 均成功，**89 passed / 0 failed**（提交 `68fd6d6`，已包含主逻辑及回归用例）。
- 随后补充的“脚本修复来源提示”代码提交 `6f77487` 的完整构建另见 [40330959387](https://github.com/CiBeDerORG/EasyTier/actions/runs/40330959387)，截至本次查询仍在队列中。
- 原生程序、镜像全流程以及 FnOS/Android 真机结果不可由上述前端结果替代。

## AUT-09 首轮构建测试证据

- GH Actions [40333677378](https://github.com/CiBeDerORG/EasyTier/actions/runs/40333677378) 中 Web build 和 `test:unit` 已成功，**97 pass / 0 fail**（提交 `73e8d4d`，包含单步诊断 UI、安全调用模块和单元测试）。
- 后续小修复 `d6bdbaa` 在切换手机时撤销风险确认；最终完整 GHCR 流水线及 FnOS/Android 真机仍须分别核验。

## AUT-10 半自动控件匹配（2026-10-26）

- `automation-locator-candidates.ts`：根据当前非截断控件树生成精确单字段与最多三字段组合候选；优先 Resource ID、无障碍描述、文字，再使用包名/类名补充消歧；返回匹配数量和排序评分，不使用 instance/index/位置 XPath。
- `UiInspectorDialog.vue`：选中控件后展示排名候选；匹配多项的候选不可点选；用户明确选择唯一候选后，才允许将原生定位器加入草稿或覆盖既有步骤。切换控件、刷新快照、设备目标指纹变化、切换定位方式或媒体源时清空相应确认。**坐标不再自动退化选中**，必须在画面尺寸和控件边界合法时主动选择并再次点击「确认使用坐标」。控件树截断或绑定漂移时禁止写入。
- **安全限制**：控件树截断或密码节点不产生候选；匹配结果只证明当前快照中的唯一性，不保证未来 UI 稳定。运行时执行器继续复核匹配数量；不新增权限、ADB 动作、持久化结构或密钥字段。
- `automation-locator-candidates.test.mjs`：覆盖 Resource ID 排序、重复文本、组合消歧、快照截断、密码节点、重复节点 ID、快照漂移、位置 XPath 和禁止 instance fallback；已加入前端 test:unit。
- **待验收**：最终提交对应的前端 CI 结果、FnOS/Android 真实 UiAutomator2 控件树；重点验证动态列表重复 ID/文字、刷新后确认清空、页面变更和横竖屏。此处只记录代码提交，**不能视作真机验收通过**。
