# FINAL V3 · 小需求计划索引（P07～P16）

> **源需求**：`../require/require-05-final-card-script-workflow.md`；**代码取舍**：`../require/require-07-product-frontend-simplification.md`。原 P01～P06 / T01～T18 保留为历史，不覆盖。

| 计划 | 名称 | 对应开发任务 | 依赖 | 状态 |
|---|---|---|---|---|
| [P07](plan-07-shared-contract.md) | 唯一画面与共享状态合同 | T19、T20 | 无（可并行预备后端空脚本调研） | 待开发 |
| [P08](plan-08-floating-shell-fullscreen.md) | 悬浮按钮、主子浮窗和全屏基础 | T21、T22、T23、T24 | P07 | 待开发 |
| [P09](plan-09-script-lifecycle.md) | 脚本新建、选中、克隆与 CAS 防丢失 | T25、T26、T27、T28 | P07；UI 浮窗依赖 P08 | 待开发 |
| [P10](plan-10-floating-timeline-editor.md) | 悬浮步骤列表、全部删除与统一编辑 | T29、T30、T31、T32 | P08 + P09 | 待开发 |
| [P11](plan-11-single-video-inspector.md) | 原视频控件拾取、属性与动作录入浮窗 | T33、T34、T35 | P08；P07 合同 | 待开发 |
| [P12](plan-12-viewer-action-leases.md) | 视频观看与动作写锁分离 | T36、T37、T38、T39 | P07；可与 P08/P09 并行 | 待开发 |
| [P13](plan-13-inline-single-trial.md) | 单步骤点击即执行与失败修复 | T40、T41、T42 | P10 + P12 + P11 | 待开发 |
| [P14](plan-14-batch-workflow.md) | 全局完整流程执行与多手机结果 | T43、T44、T45 | P09 + P12；界面 P08 | 待开发 |
| [P15](plan-15-legacy-consolidation.md) | 旧入口替代后的代码瘦身与兼容清理 | T46、T47 | P08～P14 对应能力可用后逐个执行 | 待开发 |
| [P16](plan-16-regression-acceptance.md) | 自动化测试、FnOS 真机及最终验收 | T48、T49、T50、T51 | 逐阶段跟随 P07～P15；终验依赖全部实现 | 待开发 |

## 并行波次
- **W0**：P07 业务/状态合同先行；P09 的 Rust 空脚本接口调查可同时进行，但写入必须在 P07 的修订/owner 合同确定后合并。
- **W1**：P08 浮窗框架与 P12 观看/动作 lease 协议由独立前后端 Agent 并行；严格避免共享文件同时写。
- **W2**：P09 脚本生命周期与 P11 Inspector 的新组件/纯函数并行，集成负责人安排唯一的 `AutomationCenter.vue` 接线窗口。
- **W3**：P10 浮窗时间线和全部删除/编辑；P12 Rust/Executor 联调。
- **W4**：P13 单步 + P14 整套运行（可以先开发独立控制器；涉及公共运行状态与后端交接必须串行集成）。
- **W5**：P15 逐模块替代/删除旧 UI，P16 新旧验收。QA 的自动化测试设计可从 W1 开始，但调试模式未授权测试前不执行。

## 强制禁止事项
- 不引入第二套自动化手机视频；只使用 `PhoneCard` 原实时画面和原 session。
- 不因 UI 要求跳过 owner 校验、CAS、控件唯一性、坐标检查、Journal 动作独占与幂等。
- 不在替代功能落地前删除旧组件；不把仅提交代码当作已真机验收。
