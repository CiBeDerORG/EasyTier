# 镜像构建提速轮次记录

## 2026-10-24 第四轮：将两组回归合并到单次 Cargo 调用

### 已有自动运行的实际发现

- LuCI 检查修复提交 `724d09bc113fad29d35fb5dfc3dd650ff76e75c4` 的 [自动运行 #24](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493)，[回归任务](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493/job/120820881668) 已成功。
- #24 全部任务成功并发布镜像，UTC **08:02:54–08:15:06**（北京时间 **16:02:54–16:15:06**），全流程 **12 分 12 秒**；未部署到用户设备。
- 13 项缓存边界用例通过（0.390 秒）、20 项 publication 通过（0.22 秒）、11 项 LuCI 客户端回归通过（0.38 秒）。
- 测试任务确实恢复 #23 保存的兼容缓存：738,927,398 字节，恢复用时 19 秒；664 个未改变源码输入恢复了时间戳。结束后重新记录 666 项输入并保存新版本缓存。滚动恢复已获实测证据，不能再将其表述为仅预计可用。
- 两次测试命令虽然参数和可执行文件路径相同，仍分别重新编译 `easytier` 与 `easytier-web`：第一次 3 分 13 秒，第二次 3 分 07 秒，总脚本 381 秒。此前“第二次会免编译”的预期未成立；普通日志不能进一步确定具体 Cargo 失效原因。
- #24 的原生生产构建已成功，单次编译步骤 9 分 16 秒（UTC 08:03:47–08:13:03），整个原生任务 10 分 30 秒。生产编译与测试任务继续并行。
- [#24 原生任务 120820881445](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493/job/120820881445) 实际恢复 #23 的滚动缓存 **1,346,584,466 字节**，恢复 666 项输入中的 **664 项**未改变文件时间戳。Cargo 自报编译 **9 分 14 秒**；步骤整体 9 分 16 秒包含脚本处理，不能混为同一口径。
- 原生任务成功保存新的完整程序包和 rolling target 缓存，对应源码指纹后缀 `e3c3e8becaeadadd6bcbdb12d801082a8a9716479bed2fa03a82f1b0974676b2`。这验证了源码变化后恢复前一份兼容缓存、再保存新版本的流程；没有验证项目代码可以完全免编译。
- #24 两个任务报告中的 `native-inputs.json` 已作只读比对：[原生报告 12553402443](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493/artifacts/12553402443) 记录 `recipe.runner_image_version=20261012.334.1`，[回归报告 12553527879](https://github.com/CiBeDerORG/EasyTier/actions/runs/40076510493/artifacts/12553527879) 记录 `20261023.334.3`。两边 666 项文件记录完全一致，`compiler_environment` 相同（`CARGO_BUILD_JOBS=2`、`CARGO_INCREMENTAL=0`），其他 recipe 项也相同。将测试记录中的该字段对齐后，按同一 JSON 指纹规则得到的 key 正好为原生的 `e3c3e8becaeadadd6bcbdb12d801082a8a9716479bed2fa03a82f1b0974676b2`，因此 key 差异直接由这个记录字段造成。
- 两任务的启动摘要均显示 `20261012.334.1`，与测试输入报告记录的环境值不一致。**现有证据无法解释上游记录差异，也不能据此认定实际物理镜像不同。**能确认的是缓存以指纹中记录的 `ImageVersion` 分区；后续运行是否命中仍须读取真实 key 和日志，不能仅根据恢复步骤成功或耗时短作判断。

### 本轮修正

- `test-native.sh` 改为一次 `cargo test`，在 `--` 后将 `publication` 与 `discovery::luci_client::tests` 两个筛选同时交给 libtest，按并集执行；不再给第二次 Cargo 重新编译的机会。
- 已只读核对固定版本 [Rust 1.95.0 的 libtest 参数源码](https://github.com/rust-lang/rust/blob/1.95.0/library/test/src/cli.rs) 与 [官方测试筛选说明](https://doc.rust-lang.org/rustc/tests/)，该版本支持多个筛选的并集；不修改工具链或生产优化参数。
- 合并输出保存为 `native-tests.log`，强制 pretty 格式和无颜色；逐组要求至少存在一个实际 `ok` 的测试行。保留 `pipefail` 和发布对整个回归任务成功的依赖。
- 本轮只修改测试脚本及记录，业务源码和原生构建配方未变化；#25 已验证完整原生产物缓存的精确复用，详见下文。本轮测试结果取自 #25 的真实执行，未沿用 #24 结论代替。

### 执行状态

- 已进行独立 agent 的只读交叉复核。
- 本地测试、构建、类型检查、lint、脚本验证：**未执行**；手动 Actions / 重跑：**未触发**。
- 本轮属于已观察到的重复编译修正，提交 `f37049eccfcde64df0abd59cd86f2cc365ab4b74` 已通过既有 push 事件触发 [自动运行 #25](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133)，开始时间 UTC **08:15:20**（北京时间 **16:15:20**）。
- #25 已全部完成并发布；[回归任务 120823368047](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823368047)、镜像烟测与发布任务分别确认成功。结果记录来自本轮工作流，不以 #24 或单个原生任务成功代替全流程结果。

### #25 已验证的完整原生产物复用

- [原生任务 120823367972](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823367972) 已成功，UTC **08:15:22–08:15:35**，整体仅 **13 秒**。
- 精确命中 #24 的完整程序包 `ghcr-native-v2-Linux-X64-e3c3e8becaeadadd6bcbdb12d801082a8a9716479bed2fa03a82f1b0974676b2`；实际恢复 **27,244,395 字节**，恢复步骤 **2 秒**。
- 日志确认 `REUSED_NATIVE=true`；原生环境准备、rolling target 恢复、时间戳恢复、Cargo 编译、重新保存原生产物等步骤全部跳过，日志没有 `Finished release`，本轮没有原生重编。
- manifest 的 `backend_source_sha` 保留 `724d09bc113fad29d35fb5dfc3dd650ff76e75c4`，正确表示 #24 的真实程序来源。
- #25 只改变测试脚本和文档，因此已经验证“后端输入不变可复用完整程序包”。纯前端改动尚未作为独立场景运行，不能将本次场景表述为已完成纯前端变更对照测试。
- [前端任务 120823368059](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823368059) 已成功，UTC **08:15:22–08:16:22**，耗时 **1 分钟**。

### #25 单次测试调用、冷缓存与最终发布结果

- 工作流全流程 UTC **08:15:20–08:26:16**（北京时间 **16:15:20–16:26:16**），总计 **10 分 56 秒**，所有任务成功。相对旧流程 #22 的 23 分 45 秒减少 **12 分 49 秒，约 54.0%**；不同运行的源码和缓存状态不同，不作为固定耗时承诺。
- [回归任务 120823368047](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823368047) UTC **08:15:22–08:25:21**，整体 **9 分 59 秒**。
- Python 缓存边界测试：**13 项全部通过，0.474 秒**。
- Rust 日志只有 **1 个 `Finished release`** 与 **1 个 `Running unittests`**，确认只执行一次 Cargo 编译和一次测试进程；Cargo 编译 **9 分 14 秒**，整个测试脚本 **555 秒**。
- Rust 结果：**31 passed、0 failed、0 ignored、375 filtered out，0.69 秒**；其中 publication **20 项**、LuCI **11 项**。两组实际测试行的 awk 门禁均通过，避免筛选为空也被当作通过。结果仅覆盖两组筛选并集，不代表完整 Rust 测试套件通过。
- 测试缓存日志明确为 **`Cache not found`**；本轮依赖指纹前缀为 `306a03d922df...`，而 #24 测试为 `804e1d05fe44...`，本轮使用新的兼容分区。时间戳元数据不存在，**没有恢复任何源码时间戳**。因此 **9 分 14 秒是冷缓存测试编译**，不能把 #24 的恢复记录套用到 #25，也不能称此次直接复用了测试二进制。
- 成功后重新记录 **666 项**原生输入时间戳，并保存以下新分区缓存：

```text
ghcr-test-target-v1-Linux-X64-306a03d922dfaa3fa9036a960994230dcb36869b7433dd2b243435c1381f6680-4d145bf8621664572da526e297a672070700d465b07771e4d5d26189b79ecf93-e3c3e8becaeadadd6bcbdb12d801082a8a9716479bed2fa03a82f1b0974676b2
```

- [镜像装配与烟测任务 120823584855](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120823584855) UTC **08:16:24–08:17:14**，**50 秒**，成功。装配完成后仍等待回归门禁通过才允许发布。
- [发布任务 120825356374](https://github.com/CiBeDerORG/EasyTier/actions/runs/40077317133/job/120825356374) 已成功，发布后匿名拉取并核实 `ghcr.io/cibederorg/easytier:feat-automation-center`：镜像 revision 为 `f37049eccfcde64df0abd59cd86f2cc365ab4b74`，架构 `linux/amd64`。
- 镜像索引摘要：`sha256:36c2fca0d951ea6c710d5322ff9426ade9b1306bb4765582db8242119e57e535`。
- `linux/amd64` 子摘要：`sha256:67b25dd6900631b737f2b3bb2b15cff6a4f16b266eb92d4a4859628ebd644c9d`。
- 镜像本轮组合 revision 与实际后端来源 `724d09bc113fad29d35fb5dfc3dd650ff76e75c4` 分开保留，符合完整程序复用的来源记录规则。
- 未部署到用户设备。本地测试、构建、类型检查、lint 或验证脚本未执行；手动 Actions / 重跑未触发，以上结果全部来自 push 自动 CI。

### 当前性能边界

- 已实测：#24 后端变化时滚动恢复并保存新缓存；#25 后端输入不变时精确复用原生整包、原生任务仅 13 秒；#25 两组回归只进行一次 Cargo 调用。
- #25 总耗时的主要等待是新分区测试程序冷编译 9 分 14 秒。缓存分区变化、Cargo 对项目 crate 的失效判定及测试配方变化仍可能要求编译，不能承诺每轮固定 10 分 56 秒。
- 本次没有得到“相同测试配方、相同兼容分区下的单次 Cargo 热缓存测试只需三分钟”的运行证据，不能将 #24 的两次独立编译时长转换成这一结论。
- 纯前端改动尚未单独运行对照场景；用户设备上的新版镜像行为仍待实际部署验收。

## 2026-10-24 第三轮：单次原生编译与并行回归

### 授权、基线与实际证据

- 用户在审阅慢构建原因后明确要求“你优化一下”。交付分支为 `feat/automation-center`，基线 `4c816d780703ea6f9e238701a574998b4f40a49f`。
- 构建 #20：<https://github.com/CiBeDerORG/EasyTier/actions/runs/40033901381>，总耗时约 37 分 53 秒；三轮原生编译分别 13 分 35 秒、12 分 27 秒、8 分 59 秒。
- 构建 #21：<https://github.com/CiBeDerORG/EasyTier/actions/runs/40069487917>，总耗时约 28 分 22 秒；三轮分别 7 分 59 秒、9 分 29 秒、7 分 59 秒。20 个发布回归测试实际执行 0.18 秒。
- 最近旧流程 #22：<https://github.com/CiBeDerORG/EasyTier/actions/runs/40073334116>，提交 `b51e4c025a024bab660c3b7953e7bb9a98735fe4`，UTC 07:14:11–07:37:56，总耗时 23 分 45 秒；三轮 Rust 分别 6 分 56 秒、7 分 55 秒、6 分 43 秒。
- #21 依赖缓存恢复约 1,211 MB，完全命中，末尾 `Cache up-to-date.`；因此瓶颈为项目发布编译、测试编译和 Web 特性切换，不是镜像上传。
- 基线已有后端源码变化，完整程序缓存失效合理；工作流整个文件入 hash 属于另一个过宽失效问题，不能把本次所有重编归因于显示名称。

### 本轮实现

1. 原生脚本一次 Cargo release 构建 core、cli 和 web-static。Web 实际程序保存为 `easytier-web-embed`，`easytier-web` 使用 shell `exec --no-web` 保留 API-only 入口。
2. 公开服务回归从原生构建脚本移到独立 `test_native` 任务，与前端和原生并行；`publish` 同时依赖镜像烟测和回归成功。
3. 生产发布参数不变。测试命令单独设置 opt-level=1、LTO=false、codegen-units=16，继续使用同一 musl 架构和分配器，启用镜像使用的 web-static 特性。
4. Cargo 产物通过 actions/cache 按依赖前缀恢复、源码版本精确保存。首次可读旧 release 依赖缓存作为迁移种子，禁止旧 rust-cache 的 post 清理和保存。
5. 完整原生包使用 schema 2，输入聚焦实际本地 Cargo 依赖闭包及资源链接目标，保留 SQL/proto/assets 等非 Rust 文件，去除整个工作流显示内容的失效耦合。
6. Cargo 缓存同时保存源码时间戳元数据；仅当前 Git 跟踪路径、文件模式和内容哈希全部一致时恢复时间戳。缺失、损坏或变化记录不会使变化源码沿用旧时间戳；任何选中符号链接的增加、删除或重指向都会跳过整轮恢复，防止另一份已缓存目标的旧时间戳掩盖链接语义变化。
7. 镜像烟测补充 API-only 真实启动、无 UI 目录、参数含空格、两个入口版本、HTTP 页面隔离、PID1/信号及退出码；保留 Core、部署启动接口、网页与配置生成器检查。
8. Web 的非 Windows 构建脚本显式声明 build.rs、resources、locales 输入，独立 embed 模式额外跟踪 frontend/dist；目录式 web-static 不再默认扫描整个前端包。Windows thunk 路径沿用原逻辑。

### 执行记录（提交前）

- 已阅读当前远端源码及历史运行日志，并进行独立 agent 的只读交叉审阅。
- 本地测试、构建、类型检查、lint、脚本/YAML 验证：未执行。本轮沿用仓库规定的 push 自动 CI 验证路径，未额外运行本地验证。
- 手动 Actions / CI：未触发。统一代码提交后由既有 push 事件自动构建 `linux/amd64`。
- 此处为提交前记录，当时尚未执行本轮 CI；提交后的实际结果见下节。上述 #20/#21/#22 仅为优化前基线。

### 自动 CI 实测：#23 已成功并发布镜像

- 提交：`38fa65f90bd6095e03ac7ee351991a2609d48b26`；分支：`feat/automation-center`；由 push 自动触发，无手动触发或重跑。
- 工作流：[EasyTier GHCR Image #23](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165)，所有任务成功。
- 全流程：2026-10-24 北京时间 **15:39:39–15:53:09**（UTC **07:39:39–07:53:09**），总计 **13 分 30 秒**。
- 相对最近旧流程 #22 的 23 分 45 秒，减少 **10 分 15 秒，约 43.2%**。该数字比较实际运行结果；提交、缓存与 Runner 状态并非完全相同，不作严格基准测试或固定耗时承诺。

| 环节 | #23 实测 | 结果与含义 |
|---|---|---|
| 前端任务 | 2 分 24 秒 | 成功，与原生和回归并行 |
| 原生 Cargo 发布构建 | 9 分 42 秒 | 成功，只执行一次；保留原生产 release 优化参数 |
| 原生任务整体 | 10 分 48 秒 | 包含准备、恢复/保存缓存、manifest 与上传步骤 |
| 缓存边界测试 | 13 个，0.379 秒 | `Ran 13 tests ... OK`，没有失败 |
| 测试程序首次编译 | 11 分 19 秒 | 新测试缓存未命中，使用测试专用参数 |
| 发布回归实际执行 | 20 个，0.32 秒 | 20 passed、0 failed、0 ignored；另有 379 个被 publication 过滤器排除 |
| 镜像装配与烟测任务 | 54 秒 | 成功，随后发布门禁通过 |

上述任务有并行关系，不能把各行耗时相加当作全流程耗时。此次最长等待来自首次测试程序编译；20 个测试本身耗时仅 0.32 秒。

#### 回归与测试缓存的原始日志结论

已读取 [test_native 任务 120816375902 的完整日志](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/job/120816375902)，以下时间均为 UTC：

- **07:39:50**：Python 缓存测试报告 `Ran 13 tests in 0.379s`，随后为 `OK`。范围包括本地依赖继承、target/dev 依赖、SQL/proto 资源、无关前端/工作流显示内容、有效编译配方、资源链接、链接环/越界、时间戳内容校验及链接重指向保护。
- **07:40:49**：测试缓存日志明确为 `Cache not found for input keys`，精确 key 和兼容前缀都没有命中。`.ci-cache/source-mtimes.json` 不存在，脚本按设计跳过时间戳恢复。这是首次冷缓存运行，不能写成热缓存命中。
- **07:52:08**：Cargo 报告 `Finished release profile [optimized] target(s) in 11m 19s`。
- **07:52:09**：发布回归报告 `20 passed; 0 failed; 0 ignored; 0 measured; 379 filtered out; finished in 0.32s`。该结论只覆盖本次选中的 publication 测试，不代表完整 Rust 测试套件通过。
- **07:52:09**：成功记录 **666 项**原生输入时间戳。
- **07:52:20**：`Cache saved with key: ghcr-test-target-v1-...`，测试 Cargo 产物及时间戳元数据保存成功。
- **07:52:21**：回归证据 `ghcr-regression-report-amd64` 上传成功，artifact ID `12552997637`，最终大小 42,715 字节。[证据下载入口](https://github.com/CiBeDerORG/EasyTier/actions/runs/40075008165/artifacts/12552997637)。

测试缓存完整 key：

```text
ghcr-test-target-v1-Linux-X64-804e1d05fe4455737ab251477ee41497541fe35685ee1f679250a5b0b668d77e-645a63b95130155190544c403279443365b54970be2b17ef20bcc03099315461-9e6be1dcde6a6021073b104d59c3d84fe8c43fd0d1272043c72a617c03cf2250
```

#### 镜像发布与部署状态

- 已发布并验证：`ghcr.io/cibederorg/easytier:feat-automation-center`，架构 `linux/amd64`。
- 已核实镜像索引摘要：`sha256:a9d321843566b9206a5409fddee1d6352eb962ec3b33a171f22685168f66daef`。
- 本次没有部署到用户设备。CI 烟测、回归测试通过与实机部署验收分别记录。
- 本地测试、构建、类型检查、lint、验证脚本仍为**未执行**；上述通过结果全部来自本次自动 GitHub Actions。

### 待观察与边界

- #23 已成功建立测试缓存；后续 #24 已验证 rolling restore/save 与时间戳恢复，详见第四轮记录。缓存恢复后的项目 crate 仍会重编，不能把恢复成功等同于测试二进制直接复用。
- 原生输入完全相同的变更可复用完整程序包；#25 的测试脚本变更已实测原生任务仅 13 秒且不重编，完整发布也已成功，详见第四轮。纯前端变更场景尚未单独进行对照运行，回归仍执行并约束发布。
- 现有构建脚本、git_version 宏及 Cargo 自身指纹仍可能要求部分 crate 重编，时间戳恢复不承诺完全免编。
- 指纹记录的 `ImageVersion` 参与兼容性分区；记录值变化时可能需要重新建立缓存。该机制及 #24 两任务 key 差异的直接原因已确认，但其环境记录与启动摘要不一致的上游原因尚未确定；后续运行的实际命中情况另以日志核实。
- 回归任务使用新的编译参数，#23 已证实首次没有同 profile 的缓存；实际总耗时由并行任务中最慢者决定。
- 不把原生任务成功、push 成功或 Actions 启动等同于镜像发布成功；最终分别记录。

## 2026-10-23 实施轮次

### 授权与范围

- 用户在构建耗时分析后要求“开始处理”，本轮实现重复构建消除、WASM 缓存、原生程序完整产物复用及镜像内独立页面资源。
- 当前分支：feat/node-attributes-main；修改基线：1576c8b8aad8694f663807894ad5be8028584507。
- 按改 BUG 模式执行：修改后 push，由已有 push 触发器自动构建当前部署的 amd64；不手动触发构建，不运行本地测试、构建、类型检查、lint 或验证脚本。
- 四个 agent 按前端、Rust Web、原生流水线和只读审阅分工，互不覆盖写入。

### 代码改动

1. Web 专用 CI 先构建一次 frontend-lib，再使用两个页面的 build:ci；保留单独开发构建的前置步骤。
2. WASM 使用独立 Rust 依赖缓存及完整产物缓存，精确固定 Rust 1.95.0、wasm-pack 0.15.0 和 protoc 35.1。输入指纹与产物 manifest 校验通过后，跳过 Rust/WASM 工具安装、编译和 wasm-opt。
3. 新增 web-static feature、--web-dir / EASYTIER_WEB_DIR，由原 Web HTTP 服务读取镜像内目录。动态 api_meta.js、同源 API、HTML 禁缓存、config-generator、原端口模式继续由同一服务处理。
4. 无目录或必要页面资源缺失时在启动监听器前报错；目录模式不回退旧的嵌入资源。原 embed feature 继续支持独立内嵌发布。
5. 原生 build_native 与 build_web 并行；输入完全一致时复用完整的四程序产物，并跳过原生编译环境准备。缓存不使用近似 key，缺损或校验不符时重新编译。
6. 原生输入覆盖 Cargo 包全部跟踪源码/资源、协议、迁移、翻译、锁文件、Cargo 配置、构建配方、target/allocator、固定 Rust 版本及 runner 镜像版本，排除三个页面目录。
7. 两轮原生构建对齐 easytier allocator features；Core/CLI/普通 Web 与目录版 Web 分别保存为四个现有程序入口。
8. 分支镜像使用独立 Dockerfile，分别复制程序、页面和原生 manifest。镜像 revision 标识本次组合提交，另记录原生实际源码 SHA 和输入指纹，缓存命中时不改写旧程序的版本来源。
9. 自动 push 仍只出 amd64；手动入口可选部署架构。仅文档变更跳过镜像工作流。
10. Cargo.lock 按 tower-http 0.7.1 的 fs feature 及官方 crates.io 索引补充 http-range-header 0.4.2 和相应依赖引用，没有执行 Cargo 解析或更新命令。

### 实际执行结果

- 已进行源码和 diff 的人工阅读及 agent 交叉审阅。
- 审阅后补充：生成 WASM / native manifest 前重新比较跟踪源码快照，防止准备或编译步骤改写 Cargo.lock 后错用旧 key；移除在 shared-key 模式下不会生效的 Rust dependency cache key 参数。
- 测试：未执行（改 BUG 模式）。
- 本地构建、类型检查、lint、YAML/脚本验证、缓存脚本运行：未执行（改 BUG 模式）。
- 手动 Actions / CI：未触发。
- 本轮性能收益尚未实测；历史 16 分 32 秒原生构建、约 2 分 23 秒 WASM 和 37 秒重复库构建仅用于定位，不能作为本轮已达到的结果。
- 提交后自动构建的结果以后续 Actions 页面为准；首次新输入需要建立缓存。

### 待观察范围

- 首次 amd64 自动构建：新静态目录 feature、锁文件和原生编译是否完成，镜像内四程序是否通过已有工作流检查。
- 下一次后端输入不变的页面修改：原生产物是否显示精确复用并跳过 native prepare/build，WASM 是否复用。
- Rust、协议、资源、锁文件或构建配方变化：相关缓存是否自动失效。
- 页面首页、动态 api_meta.js、配置生成器及其 JS/WASM 资源；现有同端口、拆分端口、--no-web 和 embed 模式。
- 飞牛现有 deploy 启动命令是否直接适配新版镜像。
- 镜像 SHA 与原生版本不同的情况：镜像标签及 build-manifest.json 是否清楚记录各自来源。

### 已知边界

- 缓存失效或被清理时自动重建；首次构建不能反映纯页面修改的热缓存耗时。
- 输入范围保守，部分不影响当前 target 的 Rust 包修改也会导致重建。
- GitHub 精确 key 的缓存不可覆盖；异常缓存会继续回退重建，避免使用损坏产物，可通过提升缓存 schema 换新 key。
- 从镜像取出的目录版 easytier-web-embed 需要同时携带静态页面目录；独立单文件发布继续使用 embed feature。
- 没有调整 LTO、优化级别或 Cargo 并发，也没有新增构建服务器。

## 2026-10-23 第二轮：修复 WASM 输入指纹中的目录符号链接

### 失败证据与原因

- 用户报告自动构建失败，本轮只读取已有 Actions 状态和日志，没有手动触发或重跑任务。
- [首轮构建 39951785314](https://github.com/CiBeDerORG/EasyTier/actions/runs/39951785314)，提交 3e857980：`Build Web and config generator` 在北京时间 20:14 的 `Compute config-generator WASM inputs` 步骤失败。错误为 `IsADirectoryError: [Errno 21] Is a directory: .../easytier/assets`。
- [后续构建 39952930853](https://github.com/CiBeDerORG/EasyTier/actions/runs/39952930853)，提交 fd42e60：已读取的前端任务日志在北京时间 20:37 出现相同错误。
- `easytier/assets` 是 Git mode 120000 的符号链接，指向仓库根目录 `assets`。新增的 WASM 指纹脚本无条件使用 `Path.read_bytes()`，跟随链接后尝试把目录当文件读取，导致前端准备阶段退出；该错误由上一轮加速改动引入。
- 首轮 amd64 原生四程序编译、原生 manifest 生成和完整产物缓存保存均已成功。该结果仅证明 3e857980 的原生构建及缓存保存成功；前端未编译，镜像装配、冒烟步骤和发布均被跳过。

### 修复范围

- 修复基于远端最新 c9ff7bbc04ae05cfce029629e7e30990d95dedfc，保留用户后续的网关组管理还原及中文提交规则。
- `web-build-inputs.py` 按 Git 文件类型分别处理：普通文件读取内容，符号链接读取链接目标文字，拒绝不支持的输入类型。
- 同时将链接指向的仓库内跟踪文件或目录下跟踪资源补入输入列表，包含根 `assets` 图片以及 README/LICENSE 文件链接目标；只使用 Git 跟踪清单，避免临时文件进入缓存 key。
- 目标中的符号链接继续展开并去重；链接目标超出仓库或没有可跟踪输入时明确失败，避免产生不完整的缓存指纹。
- 构建前和缓存保存前仍共用同一套输入快照逻辑。脚本自身包含在指纹输入中，修复会生成新的 WASM key。
- 本轮不修改两个 Web 程序的打包方式，管理网页及其资源保持原有构建方案。

### 执行结果与待观察项

- 已读取 Git 跟踪类型、链接目标、源码及已有 Actions 日志，并进行源码 diff 人工交叉审阅。
- 本地测试、构建、类型检查、lint、语法检查、指纹脚本运行：未执行（改 BUG 模式）。
- 手动 Actions / CI：未触发；修复 push 后按已有规则自动构建 amd64。
- 修复后自动构建结果：待确认；不得将历史原生构建成功视为本次完整镜像成功。
- 待观察：WASM 输入计算通过；首次 WASM 编译与产物缓存保存完成；前端页面打包完成；镜像装配、已有自动冒烟步骤及发布完成。

