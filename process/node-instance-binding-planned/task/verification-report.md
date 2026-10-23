# 实例绑定版本：本轮验证报告

2026-10-23。用户本轮明确授权“新版本开始 测试、部署或 push”。验证基于 `feat/node-attributes-main` 本轮工作树；部署制品与最终分支记录见 [release-record.md](release-record.md)。调试模式仍适用于后续未授权的改动轮次，本轮未手动触发 Actions/CI。

## 实际结果

|项目|结果|证据|
|---|---|---|
|Web 后端全包测试|339 通过，0 失败，2 个既有压力测试 ignored|[backend-full-final.log](artifacts/2026-10-23/logs/backend-full-final.log)|
|前端完整构建与类型检查|通过；截图文字修正后重新构建通过|[frontend-build-copy-polish.log](artifacts/2026-10-23/logs/frontend-build-copy-polish.log)|
|原节点属性映射单元用例|2/2 通过|[node-attributes-unit.log](artifacts/2026-10-23/logs/node-attributes-unit.log)|
|原节点属性浏览器回归|13/13 通过|[node-attributes-browser.log](artifacts/2026-10-23/logs/node-attributes-browser.log)|
|真实 HTTP 原 CRUD/约束回归|8/8 通过|[assets-api.log](artifacts/2026-10-23/logs/assets-api.log)|
|真实 HTTP 实例绑定/生命周期|13/13 通过|[binding-api.log](artifacts/2026-10-23/logs/binding-api.log)|
|旧库升级、重启重放、成套备份恢复|1/1 通过，含 7 类请求的跨重启重放|[recovery-result.json](artifacts/2026-10-23/recovery-result.json)|
|真实浏览器主操作链|13 组通过|[node-assets-e2e-report.json](artifacts/2026-10-23/node-assets-e2e-report.json)|
|真实浏览器冲突/失效/旧复审专项|7 组通过|[node-instance-binding-e2e-report.json](artifacts/2026-10-23/node-instance-binding-e2e-report.json)|

前端显示修正后最终嵌入制品、定点截图及部署核对另行附在本报告末尾。上述统计不将 Rust 内部断言、浏览器检查组和 HTTP 父子测试重复汇总成一个虚假的总数。

## 环境与命令

- 仓库：`/workspace/EasyTier-main-node-attributes`。Rust 使用 `/workspace/.easytier-env/activate.sh` 的工具链、protoc 与编译缓存，Node 使用已安装 pnpm，浏览器使用 `/usr/bin/chromium`。
- 数据库/API/E2E：`/workspace/.easytier-node-binding-verification` 下独立 SQLite，53087 端口及独立 config-server 端口。全部账号、实例、设备和服务均为明确的合成测试数据；原11221运行库未参与测试增删、撤权或配置变化场景。
- API 与页面分别使用独立测试节点，恢复演练另建目录和随机端口。测试账号密码、Cookie、fixture、完整数据库、客户端配置和备份不提交。

在仓库根执行：

```bash
source /workspace/.easytier-env/activate.sh
cargo test --locked -p easytier-web
pnpm --dir easytier-web/frontend build
pnpm --dir easytier-web/frontend-lib exec vitest run tests/node-attributes.spec.ts
cargo build --locked -p easytier-web --features embed
```

在 `easytier-web/frontend` 执行：

```bash
WEB_TEST_CHROMIUM=/usr/bin/chromium node --test tests/node-attributes.test.mjs
NODE_ASSETS_E2E_FIXTURE_FILE=/workspace/.easytier-node-binding-verification/fixture.json node --test --test-concurrency=1 tests/node-assets-api.test.mjs tests/node-instance-binding-api.test.mjs tests/node-instance-binding-recovery.test.mjs
NODE_ASSETS_E2E_FIXTURE_FILE=/workspace/.easytier-node-binding-verification/fixture.json node --test tests/node-assets-e2e.test.mjs
NODE_ASSETS_E2E_FIXTURE_FILE=/workspace/.easytier-node-binding-verification/fixture.json node --test tests/node-instance-binding-e2e.test.mjs
```

本轮三项 API 文件分别执行，均通过；上面给出组合执行方式。共享 API 节点的两个文件须串行，不能并发修改同一实例。恢复用例自行启动独立环境。fixture helper 的 `prepare` 使用升级前的真实旧制品；不得用新制品冒充旧版生成旧库。

## 计划场景对照

|场景|实际范围与结论|
|---|---|
|V01 角色→实例→保存|两种网关新增入口、普通设备、组内设备及页面上下文通过；迟到响应处理经代码审查，未穷举所有竞态时序|
|V02 实例分类/配置异常|普通代理和改址都为网关，无代理为普通；解析异常、混合规则和非法输入有 Rust/HTTP 覆盖|
|V03 离线/停用/持久目录|真实离线节点创建及服务管理通过；停用实例可选择，持久配置与运行观察分开；未接用户实机验证所有运行态返回|
|V04 唯一性和作用域|同节点网关实例重复占用拒绝，跨节点/账号标识隔离由 SQLite/HTTP 测试覆盖|
|V05 IPv4推导|原址代理、映射、非/24、/0及/31 /32边界、越界与自身设备规则通过|
|V06 预览过期|真实修改实例后提交返回409，保留输入；重新预览后保存成功|
|V07 重名和IP约束|同节点LAN唯一、允许overlay重复；重名选项附短ID且可分别选择|
|V08 幂等|创建/补绑/核对/改址按相同请求返回原结果；真实重启7类请求精确重放；撤权、改请求及删除后不复活均覆盖|
|V09 旧库迁移|真实旧制品生成000012库→新制品000013，6张表逐行保留，6条旧设备保持未绑定及原IPv4/IPv6/服务|
|V10 显式补绑|旧网关、直属、子设备、自身设备分别确认；旧IPv6不自动改址，转换不兼容LAN须显式处理|
|V11 同实例核对|相关配置变化/预览过期、无关配置不误失效、再次确认均有后端/API覆盖|
|V12 父子独立确认|父网关刷新不改孩子保存地址；未变规则保持可用；变更及损坏孩子不阻断父级核对|
|V13 删除/重建|缺失及同ID重建分开，旧资产/服务保留；新世代父确认不自动确认孩子|
|V14 角色/配置异常|角色变化、不可用/损坏快照状态覆盖；仍能维护元数据和服务，不自动转换角色|
|V15 自身/归属|自身唯一、继承实例overlay、固定节点/组/实例及拒绝跨scope写入均覆盖|
|V16 权限|普通账号隔离、管理员目标账号、实际撤权后关闭弹窗并清理跨账号资料；external webhook资产不可用由HTTP路由测试覆盖|
|V17 删除|节点/非空网关受限；设备按提交时服务集级联删除，不产生孤立记录|
|V18 页面与数量|显式查询、清空不匹配筛选并定位新增、多行服务展开、网关不计入实体设备数量均通过|
|V19 原功能|全包包含原config-server注册/配置流程；原属性2+13回归通过。真实预览客户端重连见部署记录；外部OIDC供应商未实测|
|V20 按需加载|代码审查确认新增资产路径无轮询；服务按展开/操作/手动刷新加载。短时静置观察结果见最终制品复核，不等同无限期监测|
|V21 页面可用性|英文操作与中文截图、390px固定操作列/整页无溢出、Unicode长度覆盖；完整键盘及读屏器审计未执行|
|V22 升级/恢复|真实旧库升级、重复启动迁移数不增加，旧备份配旧程序成功恢复登录及IPv6/服务；不对原运行库执行回滚演练|

## 发现与处理

- 新旧创建契约发生变化，原测试夹具改为“持久实例＋预览版本”，旧资料由升级前记录或明确legacy夹具生成。保留既有权限、删除级联、LAN唯一、服务协议端口、IPv6维护和幂等断言，没有跳过失败用例。
- 新测试开发期间出现契约错误码预期、中文按钮定位和持久化夹具错误，修正测试数据/断言后重跑；未因此放宽业务约束。首次前端构建未加载protoc环境导致下载失败，使用既有工具链后通过。
- 产品截图发现中文候选仍有英文占用提示、网关来源名称与长ID黏连。已补本地化并显示“网关名 · 实例名 · 短ID”，完整ID留作悬浮提示。前端构建重跑通过，最终图像以本轮收口截图为准。
- 后端本轮未因测试修改生产逻辑；新增28个测试，资产相关共45个，包含事务失败回滚、DHCP默认值/池指纹、中央分配地址作用域和部分绑定快照损坏。
- 旧复审R01固定操作、R02同名区分、R03失效与临时错误、R04返回位置、R05Unicode均已获得实际浏览器证据。R03网络故障场景通过中断一次真实GET制造失败，未mock成功响应；其他核心链路使用真实HTTP和SQLite。

## 未覆盖边界

2个原本ignored的高连接数压力测试（4097 TCP、1000真实管理连接）保持原状态。未测试外部OIDC提供商、用户OpenWrt/NAS/手机实际路由、公网转发、恶意远端RPC异常IPv4前缀注入、完整无障碍审计。新路径仅IPv4推导，旧IPv6保留验证不等于实现IPv6新绑定。没有将配置保存成功宣称为真实网络可达，也没有新增公网/Dot访问入口。

## 最终制品与部署复核

前端文字/来源分隔修正后再次完整构建并嵌入：最终二进制SHA256为`5d7c0653745d12e88e1cd1d2c6cb329ebaa224a23cd0aba7ba21c8d8d6294a5f`，`index-BOyOoWD3.js`服务内容和dist一致。真实普通/管理员登录及权限上下文通过，见[制品smoke](artifacts/2026-10-23/final-artifact-smoke.json)。

最终浏览器定点复核4/4：加载最终bundle、中文占用提示且候选仍禁选、继承来源含分隔/短ID/完整IDtitle、真实地址预览和3秒静置。静置期间新增资产请求0；这是短时观察，不是所有未来区间的保证。两张受影响截图已更新，见[最终页面报告](artifacts/2026-10-23/final-ui-smoke-report.json)。新页面合计13+7+4=24项检查；未捕获异常及清理失败均0。

原11221已升级，实际登录、原资料、前端制品、外键、SQLite完整性和原TCP客户端重连检查通过。部署检查脚本的两次参数/路径错误及恢复均如实记录，详见[交付记录](release-record.md)。完成源码及证据归档后`git diff --check`通过；文档状态更新不追加项目测试或CI。
