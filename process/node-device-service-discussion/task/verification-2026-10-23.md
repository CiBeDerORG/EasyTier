# 节点详情、设备与服务：本轮验证结果

日期：2026-10-23（Asia/Shanghai）。工作树 `/workspace/EasyTier-main-node-attributes`，分支 `feat/node-attributes-main`。用户在集中验证提议后回复“继续”，授权本轮构建、关键接口、页面 E2E 与截图；没有触发 Actions/CI、提交、push 或替换原部署。

## 结果与执行范围

|项目|实际结果|证据|
|---|---|---|
|后端既有测试|294 通过，2 项原有压力测试忽略|`cargo test --locked -p easytier-web`；[首轮日志](artifacts/2026-10-23/backend-initial.log)|
|新增资产 DB / REST|17 项通过|`cargo test --locked -p easytier-web assets::`；[日志](artifacts/2026-10-23/backend-assets.log)|
|前端构建与类型检查|通过，含最终布局/ARIA修复|`pnpm --dir easytier-web/frontend build`；[日志](artifacts/2026-10-23/frontend-build-final.log)|
|后端含页面构建|通过，含最终前端产物|`cargo build --locked -p easytier-web --features embed`；[日志](artifacts/2026-10-23/embed-final.log)|
|原节点属性映射|2 项通过|Vitest 单文件 `tests/node-attributes.spec.ts`；[日志](artifacts/2026-10-23/node-attributes-unit.log)|
|原节点页面浏览器回归|13 项通过|`node --test tests/node-attributes.test.mjs`；[日志](artifacts/2026-10-23/node-attributes-browser.log)|
|真实 HTTP API|7 组场景通过（含父测试，node:test统计8项）|[测试源码](../../../easytier-web/frontend/tests/node-assets-api.test.mjs)；[最终日志](artifacts/2026-10-23/api-final.log)|
|新页面真实 E2E|12 个顺序步骤通过；无未捕获浏览器错误、无清理失败|[测试源码](../../../easytier-web/frontend/tests/node-assets-e2e.test.mjs)；[报告](artifacts/2026-10-23/node-assets-e2e-report.json)；[日志](artifacts/2026-10-23/e2e-final.log)|
|旧库升级|旧版程序创建的隔离库升级后，4 条节点及手工属性保留；外键检查无异常|[迁移结果](artifacts/2026-10-23/migration-result.json)|
|真实 config-server TCP 注册|实际 EasyTier 客户端注册成功，观察到来源IP 127.0.0.1；地点、型号、别名、公网角色保持|[结果](artifacts/2026-10-23/config-server-result.json)|

后端 294 个既有用例和 17 个新增用例合计 311 项通过。扩展全包曾有一个新测试失败：测试复用已成功的 request_id 又改变节点，却预期404；服务依约返回409。修正该测试输入后，17 个新增测试全部定向复测通过。业务实现未为该错误预期改变。2 项原有忽略用例分别需要4097个TCP连接、1000个真实管理连接及较高FD限额，本轮未执行。

## 验证覆盖

- 数据库真实迁移、外键、多连接设置、账号/节点归属隔离；IP/CIDR、mapped IPv6、LAN唯一、自身设备唯一、固定归属及字段限制。
- 持久幂等、变更同请求内容拒绝、删除后的原请求410、并发/重启重试；服务双协议端口冲突和事务失败全回滚；设备级联当前服务，含下属资料的组/节点禁删。
- REST未登录JSON401、管理员当前会话权限和撤权、跨账号归属、external webhook禁用新资产功能、原节点删除拒绝时不拉黑。
- 管理员先选账号再选节点、网关组/类型维护、显式查询、新设备默认直属、被筛选排除时清空并定位展开、多设备展开、服务CRUD/安全链接/内置图标、离线节点管理。
- 桌面1440px和手机390px无整页横向溢出；宽设备/服务表在自己的内容区内滚动。服务图标及其他新Select有可访问名称。

## 本轮修复及截图

首轮真实E2E走完功能步骤后，布局断言发现390px视口整页宽1427px；同时发现空筛选缺少默认文字、服务图标Select缺少可访问名称。已修复网格/卡片最小宽度和表格局部滚动，补齐“全部设备/全部类型”提示及7个Select标签，重新构建并通过E2E。此前失败记录保留于运行目录的 `screenshots/before-layout-fix/`，不冒充最终效果。

- [桌面节点详情](artifacts/2026-10-23/node-assets-desktop-cn.png)
- [390px 手机节点详情](artifacts/2026-10-23/node-assets-mobile-390-cn.png)
- [390px 手机服务弹窗](artifacts/2026-10-23/node-assets-service-form-390-cn.png)

截图来自本轮构建和真实隔离数据库，使用明确标注的演示资料；没有mock资产接口。最终弹窗截图关闭动画并使用视口范围，避免将入场过渡帧或全页拼接当作稳定弹窗效果。

## 交付边界

本轮测试使用独立库、专用账号与localhost端口，没有读取/复制原演示库或更改原运行服务。没有连接用户家庭OpenWrt，也没有实现公网转发、定时心跳或自动发现。后续升级部署属于T16；原部署仍运行先前版本，当前代码未提交或push。调试模式继续有效，本轮授权不扩展为今后每轮自动测试。

独立测试服务已在验证结束后停止。证据已复制到本目录的 `artifacts/2026-10-23/`，未归档测试账号凭据、数据库或会话内容。真实OIDC供应商登录、用户OpenWrt实机接入、原部署备份与恢复本轮未执行；本地TCP注册不能替代这些场景。

后续交付更新：用户在验证结束后明确要求“先push”，本报告随当前分支代码提交；前述“未提交/push”描述验证结束当时状态，提交及远端结果以 Git 记录为准。本次推送不追加测试或部署。
