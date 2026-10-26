# AUT-08 · 自动化中心端到端验收与发布门禁

适用分支：`feat/automation-center`。
维护日期：2026-10-26。

> **真实状态：模拟设备回归与镜像 CI 可独立通过；本轮未连接用户 FnOS 或 Android 真机，不得将本文件中的任何真机项目填为“通过”。**
> AUT-00 的硬件兼容基线仍需另行核实：[`automation-aut00-acceptance.md`](./automation-aut00-acceptance.md)。

## 1. 发布结论规则

| 门禁 | 条件 | 现状 |
| --- | --- | --- |
| G1 Web / Rust | 提交对应 Web 前端构建、单测、Rust 原生回归、镜像发布与 smoke 测试均成功 | **持续检查**；不能以 Web 子任务成功代表整个 GHCR 工作流成功 |
| G2 Automation Executor | amd64 单独镜像构建成功、鉴权 HTTP smoke、真实 SQLite journal + mock Android RPC 安全回归通过 | **CI 2026-10-26 已通过**（应再次核对部署镜像摘要） |
| G3 FnOS 集成 | 主服务、执行器 token / callback / 账号鉴权、重启后任务恢复与数据持久化检查通过 | 待真机 / FnOS |
| G4 Android 真机 | 至少 Android 17 和一个不同 Android 版本；A00-01～A00-10 + 本文件 E08-01～E08-16 | 待真机 |
| G5 风险接受 | 未确认动作不自动重试；业务成功只有通过明确的末尾断言才确认；失败可修复但不能覆盖历史 | 自动测试覆盖，待现场复核 |
| G6 发布 | G1～G5 均满足，记录部署镜像 digest、验证人员/日期、剩余缺陷及可回退版本 | **未放行** |

**判定规则**：G1～G6 中任意一项未通过都不可宣布“自动化中心已完成端到端验收”。必要时可单独发布候选版，但必须明确标注“待真机验收”，不得标记为正式通过。

## 2. 环境准备（测试专用）

- FnOS 安装 EasyTier 主服务和独立的 `easytier-automation-executor`；使用 Linux amd64 的 CI 产物；双方配置来自兼容协议的确定提交。
- 预备两台运行不同 Android 版本的手机（至少一台 Android 17），另备一台可模拟 ADB 离线或未授权的手机；统一使用可重置的**测试应用**，不能对真实账户发送消息、转账或支付。
- 记录设备 ID、型号、ROM / Android 版本、分辨率及服务端记录的 `target_fingerprint`；禁止上传 ADB pairing code、token、账户密码及未脱敏的控件树。
- 每台手机单独确认有线或无线 ADB 连接状态。UiAutomator2 只有在实际需要控件操作且获得操作授权时才初始化。
- 在测试环境中配置一个有明显成功提示控件和两条不同分支的测试应用流程，避免使用不可逆操作。

只读现场自查（在 FnOS 终端上执行；不要公开 token）：

```bash
sudo docker inspect easytier-automation-executor \
  --format '镜像={{.Config.Image}} 摘要={{.Image}} 启动={{.State.StartedAt}} OOM={{.State.OOMKilled}}'
sudo docker exec easytier-automation-executor adb devices -l
sudo docker logs --tail 120 easytier-automation-executor
```

主服务镜像摘要应同时从 FnOS 实际使用的容器读取并保存；以 `docker inspect` 实际结果为准，不假设容器名和配置。不要使用 `adb connect`、`adb install` 等有副作用的命令作为只读检查。

## 3. 端到端人工验证矩阵

| 编号 | 场景 | 操作 / 预期 | 证据 |
| --- | --- | --- | --- |
| E08-01 | 初始诊断 | 查询三台手机的 ADB 和控件状态；离线一台超时不阻塞其他设备 | 逐台检查结果、时间、设备摘要 |
| E08-02 | 控件初始化 | 对在线手机显式准备 UiAutomator2；再次查询应显示 ready，不能在状态查询阶段静默安装 | 初始和最终组件状态 |
| E08-03 | 真实画面录制 | 在测试应用中录制点击、长按、滑动、拖拽；长按必须生成 long_press，位移动作必须有滑动/拖拽确认 | 步骤草稿与真机录制动作 |
| E08-04 | 控件拾取 | 控件树查唯一 Resource ID / 描述；回填录制步骤后确保动作 UUID 和原参数不变 | 拾取前后定位条件 |
| E08-05 | 嵌套脚本 | 创建 If/Else、移动/删除分支内步骤；跨分支非法删除必须拒绝，旧脚本扁平协议保持不变 | 脚本结构及保存结果 |
| E08-06 | 草稿连续性 | 拾取、录制和手动编辑的步骤合并；切 tab 保留；切换已保存脚本时需确认丢弃未保存数据 | 草稿步骤数量和顺序 |
| E08-07 | 单机试跑 | 仅用测试手机发起带最终断言的单机试跑；不需要自动启动镜像投屏 | 执行任务 ID、逐步骤状态 |
| E08-08 | 批量预检 | 一台在线 ready、一台在线但组件未准备、一台离线；每台异步显示独立状态，只能勾选 ready 设备 | 预检截图和服务端任务选择 |
| E08-09 | ADB-only 脚本 | 仅包含 `key`、`launch_app` 的脚本在组件未准备时仍可试跑 | 预检结果和执行结果 |
| E08-10 | 并行执行 | 两台在线手机并行，走不同 If/Else 分支；确认分支互不串扰、未命中分支不执行 | 两台步骤状态，跳过源 UUID |
| E08-11 | 业务验证 | 一台末尾断言通过，一台末尾断言失败；分别显示「业务已验证」「验证失败」，汇总不得误报全部成功 | 每台最终断言及汇总状态 |
| E08-12 | 旧记录 | 历史上只有普通断言、没有 `business_verification` 标记的运行只显示「业务未验证」 | 历史运行快照和业务状态 |
| E08-13 | 失败诊断 | 从失败设备进入失败步骤，展示设备 ID、原脚本修订、步骤 UUID、错误、时刻和现有证据；没有截图不能伪造链接 | 失败诊断区域 |
| E08-14 | 修订冲突修复 | 原脚本执行后再人为保存新修订，点击历史失败步骤只能从旧快照建立独立草稿，不得覆盖新修订 | 新旧脚本 ID / 修订和源运行 ID |
| E08-15 | 结果未知 | 在已发出动作时断开 ADB / 执行器，确认 `outcome_unknown` 不会自动重放；手工核实并人工处理独占释放 | 操作调用次数、清理确认状态 |
| E08-16 | FnOS 重启及发布 | 重启主服务/执行器，验证 SQLite 历史不丢失、旧结果不变、有效运行身份和 token 权限正确；锁定两镜像 digest | 重启前后历史、镜像摘要 |

测试 E08-03、E08-10、E08-15 必须在**专用测试应用**内实施；尤其 E08-15 涉及断连，不应在商业操作或实际付款环境中复现。

## 4. 自动化回归覆盖及其边界

- Web 前端：`easytier-web/frontend/tests/automation-closure-integration.test.mjs`。用真实生产的 TypeScript 模块串起录制、草稿、树状脚本、目标更新、模拟设备预检、模拟业务验证、版本安全修复，覆盖正向与异常流程。**不覆盖真实浏览器、网络、安卓或 FnOS**。
- Python 执行器：`automation-executor/tests/test_device_exit.py::RuntimeTests.test_aut08_two_phone_success_and_terminal_assert_failure`。通过真实调度器和 SQLite journal，使用 mock 手机 RPC 验证两台设备不同分支以及仅一台最后断言失败。**不覆盖真机 UIAutomator2 或真实 ADB 无线链路**。
- Rust 原生：业务标记验证、持久化结果和执行器派发协议依赖已有 `business_verification_contract_tests`、`business_verification_run_tests`，应同时纳入 native CI。
- 前端单元测试执行入口：`pnpm --dir easytier-web/frontend test:unit`。独立执行器镜像 CI 内通过 `python -m unittest discover -s /automation-tests -p 'test_*.py' -v` 运行 Python 安全回归。
- 任何一个仅有模拟手机的成功测试都不能替代 G3 / G4。所有实际的调试设备、连接与截图必须由授权现场测试人员操作。

## 5. 结果填写模板

| 项目 | 现场填写 |
| --- | --- |
| FnOS 版本 / 主机架构 | 待验证 |
| EasyTier 主服务提交 / 镜像 digest | 待验证 |
| Automation Executor 提交 / 镜像 digest | 待验证 |
| Android 17 手机型号 / ROM | 待验证 |
| 另一 Android 手机型号 / 版本 | 待验证 |
| 模拟离线手机 | 待验证 |
| G1 Web/Rust | 待验证完整 CI |
| G2 Executor | CI 已有成功记录，待核部署摘要 |
| G3 FnOS | 未测 |
| G4 Android | 未测 |
| G5 安全 | 部分自动回归，待真机 |
| G6 发布决定 | **未放行** |
| 测试人员 / 时间 | 待填写 |
| 证据位置（脱敏） | 待填写 |
| 阻断缺陷 / 修复提交 | 待填写 |

## 6. 回退标准

- **立刻停止验收、暂缓发布**：出现跨手机操作、越权、密码泄漏、未知结果被重发、断言失败仍显示业务验证成功、失败修复覆盖历史或错误脚本等问题。
- 保留原 SQLite 数据和执行记录，停止发起新任务，确认独占/清理状态后，再部署上一个通过镜像摘要的版本。
- 回退不得以数据库删除或重置执行记录作为捷径；如需模式迁移，先进行备份且遵循当前版本的迁移约束。
