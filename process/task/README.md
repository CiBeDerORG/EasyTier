# 任务看板

更新日期：2026-10-21。协调者维护此摘要；状态、负责人、依赖及验证证据以任务明细 YAML 与执行记录为准，见 [协作规则](../README.md)。当前为手工维护的 Markdown 看板，无自动同步服务。

## 当前需求

| 主题 | 需求分析 | 实施计划 | 当前阶段 |
| --- | --- | --- | --- |
| Web 用户管理 | [需求与验收](../../docs/requirements/web-user-management.md) | [PLAN-WUM](../plan/web-user-management.md) | M2 并行开发：契约已交付，权限回归与存储/客户端/组件开发推进；D-01 已按用户选择 A 解除 |

## 状态概览

| backlog | ready | in_progress | blocked | in_review | done | cancelled |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | 0 | 4 | 3 | 0 | 4 | 0 |

`ready` 表示可认领，不表示已开发；此处 `blocked` 均为等待列出的前置任务。WUM-001 契约已交付，动态功能尚未完成；D-01 已解除：保留旧账号登录，增加明确改密提示；UA-12 仍需实际升级验证。

## 任务明细

主题入口：[Web 用户管理任务文档](web-user-management/README.md)。每项任务已单独记录执行步骤、验收与交付清单；本看板保留统一状态摘要。

| 任务与明细 | 状态 | 当前负责人 | 直接前置任务 |
| --- | --- | --- | --- |
| [WUM-001 范围与接口契约](web-user-management/wum-001-contract.md) | `done` | `/root/wum001` | 无 |
| [WUM-002 按用户查询权限](web-user-management/wum-002-permissions.md) | `done` | `/root/wum002` | 无 |
| [WUM-003 账号存储与数据库迁移](web-user-management/wum-003-storage.md) | `in_progress` | `/root/wum003` | WUM-001 |
| [WUM-004 认证与会话生命周期](web-user-management/wum-004-auth-lifecycle.md) | `in_progress` | `/root/wum004` | WUM-002、WUM-003 |
| [WUM-005 账号管理业务与恢复](web-user-management/wum-005-account-service.md) | `in_progress` | `/root/wum005` | WUM-003 |
| [WUM-006 管理 API 与后端接线](web-user-management/wum-006-admin-api.md) | `blocked` | 未认领 | WUM-004、WUM-005 |
| [WUM-007 Web HTTP 客户端与错误语义](web-user-management/wum-007-web-client.md) | `done` | `/root/wum007` | WUM-001 |
| [WUM-008 用户管理组件与文案](web-user-management/wum-008-user-interface.md) | `done` | `/root/wum008` | WUM-001 |
| [WUM-009 前端导航与身份流程接线](web-user-management/wum-009-web-integration.md) | `in_progress` | `/root/wum009` | WUM-007、WUM-008 |
| [WUM-010 真实前后端联调](web-user-management/wum-010-full-stack-integration.md) | `blocked` | 未认领 | WUM-006、WUM-009 |
| [WUM-011 旧库升级与交付验收](web-user-management/wum-011-upgrade-acceptance.md) | `blocked` | 未认领 | WUM-010 |

## 并行安排与联调

- 起始可并行：WUM-001 范围/契约与 WUM-002 权限查询修复。
- 契约交付后可并行：WUM-003 存储、WUM-007 HTTP 客户端、WUM-008 独立组件。
- 存储交付且权限修复完成后：WUM-004 认证与 WUM-005 业务服务可并行；前端继续推进。
- WUM-006 后端接线与 WUM-009 前端接线可分别进行；两者完成后进入 WUM-010 真实联调。
- WUM-011 承接旧库升级、恢复与交付验收。依赖图和文件单一写入者详见 [计划](../plan/web-user-management.md)。

认领时由协调者同步明细和看板，填写实际 agent/负责人；不要把建议职责当成已分配身份。新增任务使用 [任务模板](template.md)，保留编号和历史证据。
