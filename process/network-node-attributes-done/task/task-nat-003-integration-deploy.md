---
id: NAT-003
feature: network-node-attributes
status: done
owner: /root
depends_on: [NAT-001, NAT-002]
dependency_gate: integration-and-deployment
plan: ../plan/plan-nat-003-integration-deploy.md
requirements: [NAT-01, NAT-03, NAT-05]
acceptance: [NAA-04, NAA-05, NAA-06]
updated_at: 2026-10-22
---

# NAT-003 IP 接入及联调部署

[任务看板](README.md) · [实施计划](../plan/plan-nat-003-integration-deploy.md)

## 独占写入范围

- `easytier-web/src/client_manager/mod.rs`
- `easytier-web/src/client_manager/session.rs`
- `easytier-web/src/client_manager/session/webhook_validation.rs`
- `easytier-web/src/client_manager/storage.rs`
- `easytier-web/src/client_manager/ip_location.rs`
- `easytier-web/src/client_manager/session/connection_tests.rs`
- 本次独立部署的运行配置、证据文件和需求根汇总；运行数据放在源码之外。

DB/REST 和前端问题交回对应负责人，不与其同时写文件。require/plan/task 文档由文档 agent 首次交接后，才由 root 更新状态。

## 可与实现并行的前置项

- [x] 以 main 为基线移植 IP 生产代码及适用测试，去除用户管理特有测试身份结构依赖。
- [x] 从成功认证的 TCP/UDP 连接观察来源 IP，本地定位并交由 NAT-001 的 DB 方法持久化。
- [x] 保证同一节点快照写入顺序和完整性，连接断开或写入失败不影响原认证/配置路径；不新增定时任务。
- [x] 准备独立演示数据库、端口和运行方式，记录部署数据与真实节点的区别。

## 依赖 NAT-001、NAT-002 交付后执行

- [x] 统一核对接口、类型和迁移，执行数据库/API/连接场景验证以及前端构建和交互检查，记录实际结果。
- [x] 验证心跳、重连和 IP 更新后手填值不变；覆盖非公网、定位无结果、定位不可用、数据库错误及旧快照顺序。
- [x] 使用本次前端产物构建 embed 后端，在独立端口启动，确认原登录和设备列表操作可用。
- [x] 使用真实后端完成打开编辑、保存、页面刷新复查、取消和清空；避免以浏览器测试夹具替代此项。
- [x] 提供设备卡片属性、编辑弹窗和窄屏截图，标明演示数据；记录访问地址与运行方式。
- [x] 对照全部 NAA 验收项更新根汇总、任务进度和未解决问题，向用户交付部署与截图。

前端构建完成后再运行 `cargo build -p easytier-web --features embed`，保证嵌入本次页面。测试和构建由 root 协调，复验仅针对新增变化、失败或未解决问题。未通过门禁时保持任务进行中，明确具体阻塞。

## 当前进度与问题

状态：已完成。真实客户端通过 TCP 直连部署程序；页面保存、取消、清空、重载、服务器重启与客户端重连均已验证，三张截图已检查。来源 IP 为本地回环 `127.0.0.1`，手填属性标注演示值；未现场验证用户实际 OpenWrt 的公网接入。内置 Country 库精度限制见[验收与部署说明](../verification.md)。
