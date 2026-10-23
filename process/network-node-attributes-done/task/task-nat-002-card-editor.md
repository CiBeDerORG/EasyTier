---
id: NAT-002
feature: network-node-attributes
status: done
owner: /root/node_attributes_frontend
depends_on: []
parallel_group: implementation-contract-v1
plan: ../plan/plan-nat-002-card-editor.md
requirements: [NAT-02, NAT-04]
acceptance: [NAA-03]
updated_at: 2026-10-22
---

# NAT-002 原列表属性编辑

[任务看板](README.md) · [实施计划](../plan/plan-nat-002-card-editor.md)

## 独占写入范围

- `easytier-web/frontend/src/components/DeviceList.vue`
- `easytier-web/frontend/src/components/DeviceAttributesDialog.vue`
- `easytier-web/frontend/src/components/DeviceDetails.vue`：仅增加已填写地点、型号的详情行。
- `easytier-web/frontend/src/modules/api.ts`
- `easytier-web/frontend-lib/src/modules/utils.ts`
- `easytier-web/frontend-lib/src/locales/cn.yaml`
- `easytier-web/frontend-lib/src/locales/en.yaml`
- `easytier-web/frontend-lib/tests/node-attributes.spec.ts`
- `easytier-web/frontend/tests/node-attributes.test.mjs`

只读 Rust API 核对契约，不写后端文件。最后一项浏览器测试可由前端子 agent 独占编写，前端负责人不得同时写该文件；其他文件仍由前端负责人维护。

## 工作项

- [x] 保留设备响应八个属性并新增手填更新方法，不改变现有 API 方法。
- [x] 原设备单项增加编辑属性入口和地点/型号展示，保留已有操作与视图。
- [x] 弹窗仅编辑两个字段，自动属性只读，补充中英提示和空值状态。
- [x] 本地草稿隔离列表刷新；取消不保存；提交防重复，失败保留草稿，关闭后忽略迟到响应。
- [x] 校验规则与后端一致，空串可清空，保存采用服务端标准化结果。
- [x] 添加针对性的类型映射、编辑交互和窄屏检查，记录原操作回归结果。

## 交付与门禁

与 NAT-001 根据[固定契约](../require/README.md)并行。先确认前端构建和针对性测试可运行，再交给 root 联调；真实 API 尚未通过时不得称为端到端验收完成。固定响应的浏览器夹具只作交互验证，最终截图必须在 NAT-003 实际部署后重拍。

构建入口为 `pnpm --dir easytier-web/frontend build`；浏览器测试文件使用现有 Node/Playwright 方式执行，记录命令、环境及结果。前端库新增测试通过现有 Vitest 配置执行，避免重复构建与无关全量检查。

## 当前进度与问题

状态：已完成。前端构建、2 项映射测试、10 项浏览器场景均通过；移动端测试初次因两个已有 summary 元素导致定位歧义，收窄选择器后定向复测通过。真实持久化、桌面及手机截图见[验收记录](../verification.md)。
