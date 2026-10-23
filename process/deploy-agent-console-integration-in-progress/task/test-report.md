# 验证记录

## 2026-10-23 · 第 1 轮：分支合并

- 范围：`bc792224` 合入 `66587dac`，保留目标分支布局、权限和迁移，恢复 Agent 路由及手动导航行为。
- 本轮测试：未执行，仍在合并和主题适配阶段。
- 历史记录：旧基线 Agent 的 Go、Rust、浏览器和 HTTPS 联调曾通过；不代表本次合并后的代码已通过。
- 待测：迁移兼容性、前后端构建、Agent 联调、中英文、浅色/深色、窄屏与手动请求约束。
- 已知边界：未在真实 fnOS/OpenWrt 硬件验收；自动化中心仅规划入口，尚无手机控制后端。

## 2026-10-23 · 第 2 轮：主题与入口适配

- 范围：Agent 页面改为控制台公共布局与主题变量；中英文文案、顶部手动刷新、自动化中心准备入口；新增旧 Agent 数据库升级回归。
- 前端：完整 `pnpm build` 通过；修正移动端顶部刷新按钮文字后 `pnpm build:ci` 再次通过。构建仍提示既有大分包、过期 Browserslist 数据与 pnpm 配置提示，未在本需求中扩大到依赖升级。
- Rust：首次使用 `--lib` 因该包无 library target 未执行测试；改用 `cargo test -p easytier-web --bin easytier-web`，371 项通过、2 项忽略。包含 Agent、节点属性、资产、服务导航及三条数据库迁移路径。
- 浏览器：首次测试在视口切换动画未结束时报告窄屏溢出；检查稳定截图确认布局后，将测试与项目现有响应式测试一致设为 reduced-motion。另修复实际发现的顶部刷新按钮窄屏文字换行。最终 Agent 页面两项测试通过（含 4 种语言/主题组合、390px 页面与弹窗、静置 60 秒无请求）。
- 原页面回归：`dashboard.test.mjs` 选定的 responsive layouts、external Console、merged workspace 三项通过，覆盖现有导航/地图入口、权限与自动刷新偏好。
- 截图检查：已查看 Agent 桌面/移动端及自动化中心英文暗色桌面截图；产物在 `/workspace/artifacts/console-agent/`。截图含测试夹具，不是实际已连接设备。
- 待测：嵌入前端的 Web 二进制构建及真实 HTTPS Web–Agent 安装联调。
- 未执行：真实 fnOS/OpenWrt 硬件验收、手机控制验收（尚无设备且手机后端未实现）。

## 2026-10-23 · 第 3 轮：最终联调与远端同步

- `cargo build -p easytier-web --features embed` 通过，二进制包含最终前端产物。
- `DEPLOY_REPO=/workspace/deploy node tests/deploy-agent-integration.mjs` 通过：真实 TLS、证书/凭证/主机名校验、租户隔离、服务发现、经 deploy.sh 执行隔离夹具安装、持久化去重、重启、日志及设备身份绑定。未安装真实应用或更改宿主机服务。
- 提交前远端新增 `9abf35eb`，已以 `3ad2f953` 合入其发布验证文档；本次远端增量只有文档，未重复运行代码测试。
- 最终 `git diff --check` 通过；前端与后端验证范围见上两轮。真实硬件和手机控制验收仍未执行。
- 截图输出：`/workspace/artifacts/console-agent/`；部署包：`/workspace/artifacts/deploy-agent-v1-linux.tar.gz`。这些本地产物未被当作 GitHub 已发布版本。
