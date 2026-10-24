---
id: ODS-T13
title: 自动化目标改读手机 ADB 服务
owner: null
status: todo
depends_on: [ODS-T02]
integration_dependencies: []
requirements: [ODS-R06]
plan: PLAN-ODS-08
---

# ODS-T13 自动化目标改读手机 ADB 服务

## 目标、输入与输出

落实[ODS-R06](../require/require-ods-06-automation-service-source.md)及[PLAN-ODS-08](../plan/plan-ods-08-automation-source.md)。输入是 T02 的手机/结构化 ADB 服务及唯一性字段、[公共契约](../plan/contracts/v1.md)和基线 `dbbade81`。输出是统一的主机列表与目标解析：服务身份、有效端口、版本、绑定状态和明确不可用原因。文档作者不等于实施负责人。

## 独占拟改范围与依据

- `easytier-web/src/db/automation/mod.rs`：`list_devices`、`device_json`、`target`、`fingerprint`、`ensure_route_unambiguous`、`save_connection`。已核实这些入口读取独立配置，部分含 5555 默认值。
- `easytier-web/src/restful/automation/devices.rs`：`list`、`connection`、`prepare`，替换端口来源并处理旧请求兼容。

不写两个 `runs.rs`、发现/服务存储、迁移和前端。共享 DTO、执行器协议、模块注册由 T01/T19 负责；必要调整提交接线清单，不抢写。

## 实施步骤

1. 按账号权限选择手机且唯一有效 ADB 服务；无服务、非 ADB、多候选返回契约规定的缺失/冲突，不默选端口。旧连接表不兜底、不生成服务，也不以其 enabled 过滤真实服务来源。
2. 所有准备、投屏及脚本目标复用同一解析；地址取有效组网绑定，端口取服务。将 `adb_service_id`、`endpoint_revision` 及真实路由纳入指纹，展示名变化不改变目标。
3. 取消独立端口写入；旧客户端提交不能覆盖权威服务或静默保留第二来源。具体拒绝/兼容响应遵循公共契约。
4. 路由冲突检查使用新目标，保持账号隔离及旧占用保护。服务删除、身份变化或绑定失效及时拒绝旧目标，不把公网入口冒充组网目标。

## 完成条件与交接

ODS-R06-A01 至 A05 在实现层有明确对应；不存在默认 5555 或旧表回退路径。向 T14 交接响应与不可用原因，向 T15/T17 交接目标解析和版本语义，向 T19 交接协议变更。T15 只能调用本任务入口，不修改本文件。

## 后续验证

覆盖服务/旧端口不一致、旧 enabled=false、无服务、多候选、账号切换、改名及端口变更；验证准备和会话解析一致。测试仅在后续获授权时进行，结果交 T20。本轮仅文档，未执行测试、构建或 CI。
