---
id: T08
status: verified
owner: frontend-client
depends_on: [T01]
updated_at: 2026-10-23
---

# T08 前端类型与资产API客户端

状态：已实现并完成本轮授权验证，实际范围及未覆盖项见[验证报告](verification-report.md)。归属[P04](../plan/plan-04-frontend-contract-and-editors.md)，唯一写入负责人为前端客户端agent。

> 下面的原交接/验收设计保留规划时措辞；“未执行/待授权”不再代表最新状态。当前结果以本页状态和验证报告为准。

## 输入与文件所有权

- 输入：T01冻结的[v2契约](../plan/contracts/v2.md)、现有错误/幂等语义及[前端需求](../require/require-03-frontend-workflows.md)。
- 独占：`easytier-web/frontend/src/types/node-assets.ts`、`easytier-web/frontend/src/modules/api.ts`；必要时修改`easytier-web/frontend/src/composables/useAssetMutation.ts`。
- 不修改表单、页面、locale或旧`WebRemoteClient`认证行为。T09–T11不得在并行阶段写上述文件。

## 工作内容与输出

1. 增加角色、持久配置候选、规则、地址预览、绑定状态和读写DTO，使用明确的判别联合区分网关结果和普通设备结果；旧资产字段按兼容方案继续可读。
2. 提供契约方法`listAssetNetworkInstances`、`previewAssetAddress`、`previewAssetBinding`、`applyAssetBinding`、`updateAssetAddress`，并扩展相关CRUD；准确传递`owner_user_id + machine_id`范围，按契约编码实例/规则标识。网关/直属补绑及核对走binding preview与binding PUT；旧下挂/自身设备转为继承来源及地址修正走instance-address preview与devices/{id}/address，每次apply携带request_id。首次预览不要求已有preview_version，保存使用预览响应版本。
3. 公共候选只接受脱敏摘要，不返回完整网络配置。统一表示可选性、不可选原因、配置指纹、IPv4来源及`legacy_unbound/current/changed/missing/replaced/unavailable`状态。
4. 保存继续沿用同一cookie传输及401处理；请求失败转成既有AssetError结构，配置变化/占用冲突具备稳定错误码及字段定位。
5. 保留`useAssetMutation`同一次新增重试复用request_id、请求结果不确定时保持原payload的机制；如新请求必须适配，仅在本任务修改并给出组件使用约束。

## 交接与验收场景

- 向T09/T10/T11交付类型及方法清单、输入输出示例、状态/错误码表和幂等用法；组件不得直接拼URL或复制DTO。
- 重点场景：管理员目标账号不丢失；同节点不同实例标识正确；网关与设备响应分流；候选读取失败不变为空列表；补绑/核对请求与普通资料编辑区别明确；重试不会创建第二份资产。
- 这些是后续验收条件，本轮不执行测试、类型检查、构建或CI。真实接口兼容由T12接线、T13授权验证；任务完成记录不得冒充运行验证通过。

## 2026-10-23 实施交接

types/node-assets.ts与ApiClient已按v2接入，useAssetMutation覆盖POST和PUT的原请求重试。

以上源码交接记录保留当时阶段。2026-10-23已完成T13授权验证及当前云环境部署，实际覆盖和边界见[验证报告](verification-report.md)与[交付记录](release-record.md)。
