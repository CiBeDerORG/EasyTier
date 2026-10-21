---
id: WUM-007
feature: web-user-management
status: blocked
owner: null
depends_on: ["WUM-001"]
write_scope: ["easytier-web/frontend/src/modules/api.ts","easytier-web/frontend/src/modules/userManagement.ts","easytier-web/frontend/tests/user-management-client.test.mjs"]
requirements: ["UM-01","UM-02","UM-03","UM-04","UM-05","UM-06","UM-07","UM-08","UM-09"]
acceptance: ["UA-02","UA-03","UA-10","UA-11","UA-13"]
updated_at: "2026-10-21"
---

# WUM-007：Web HTTP 客户端与错误语义

需求：[Web 用户管理](../../../docs/requirements/web-user-management.md)。计划：[PLAN-WUM](../../plan/web-user-management.md)。[返回看板](../README.md)。

## 目标与边界

实现用户管理 HTTP/DTO、当前身份和注册策略访问；修正相关错误表达与敏感响应日志。新增路径为建议；不改路由或页面，不加入网络 RemoteClient。

## 输入与输出契约

输入为 WUM-001 HTTP 与状态契约。输出类型化请求/响应、安全错误和调用方法供 WUM-009 组合；与 WUM-008 仅共享已定义契约，无需等待其组件文件。

## 执行步骤

1. 对照 WUM-001 契约梳理现有 ApiClient 的认证、密码输入和错误处理路径，逐项列出需新增或复用的 HTTP 方法、DTO 与可空字段。
2. 在所属模块中实现调用与错误映射，集中处理凭据响应和日志；向 WUM-009 提供稳定的调用接口，不等待 WUM-008 的组件实现。
3. 在本任务测试文件中准备脱敏的成功、冲突、拒绝访问和网络失败响应，验证请求与响应适配；新增测试使用实际可执行的独立入口，公共脚本变更交由 WUM-010 汇总。
4. 在 frontend 目录执行现有 `pnpm build`，运行本项针对性检查，并记录给页面接线方的用法、错误分支和兼容限制。

## 实施与验收

- [ ] DTO 保持 snake_case 契约及旧密码输入兼容；401、403、网络失败可区分，不把所有失败归为未登录。
- [ ] 创建/重置凭据不进入完整 Axios 日志、URL、localStorage 或通用提示；不改变其他业务接口的无关行为。
- [ ] 通过类型/构建检查和可执行的客户端针对性验证，覆盖错误映射、请求参数与敏感日志；明确 mock 不能证明后端授权。
- [ ] 新测试可用明确命令独立运行；固定公共 runner 的扩展由 WUM-010 汇总，不以未运行的文件作为测试证据。

## 交付清单

- HTTP 方法与 DTO 对照表、契约版本和 WUM-009 可使用的错误处理说明。
- 所属客户端实现与针对性测试文件；测试响应使用虚构数据，不附带真实凭据。
- 实际构建/测试命令、结果和未覆盖范围，以及交给 WUM-010 的测试入口接入说明。

## 阻塞与交接

等待 WUM-001。可与 WUM-003/008 并行。api.ts 与新 userManagement.ts 由本项独占；全局前端入口由 WUM-009 修改。

## 执行记录

- 2026-10-21：仅完成任务规划，尚未认领或执行；未运行本任务的实现测试。
