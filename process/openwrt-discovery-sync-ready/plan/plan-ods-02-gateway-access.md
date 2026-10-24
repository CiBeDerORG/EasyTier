# PLAN-ODS-02 网关授权设置与 LuCI 接入

关联需求：[ODS-R01](../require/require-ods-01-gateway-access.md)。当前状态：待实施；公共接口、文件和组件均按 [契约 v1](contracts/v1.md) 冻结后实现。

## 拆成三个小步骤

1. **网关配置接口**：[ODS-T03](../task/task-ods-03-gateway-access-api.md) 按网关自身资产设备读写发现地址、密钥及配置状态，检查用户、节点、组、类型和角色，不让普通资产响应携带明文。
2. **LuCI 客户端**：[ODS-T04](../task/task-ods-04-luci-rpc-client.md) 完成固定账号登录、四项发现调用、分层错误解析和有界超时；调用者明确决定 `scan` 或 `list`，客户端不自行安装、扫描或管理授权。
3. **配置与入口组件**：[ODS-T05](../task/task-ods-05-gateway-access-ui.md) 提供基本信息配置区及设备行的两个操作入口，输出上下文事件；同步差异清单归 ODS-T09，ADB 结果归 ODS-T12。

## 依赖与并行边界

| 任务 | 开发前置 | 仅联调依赖 | 可独立工作的范围 |
| --- | --- | --- | --- |
| ODS-T03 | ODS-T02 | ODS-T04 | 配置持久化与授权处理，客户端用契约替身 |
| ODS-T04 | ODS-T01 | ODS-T03 | RPCD 传输与解析，通过注入配置独立开发 |
| ODS-T05 | ODS-T01 | ODS-T03 | 独立 Vue 组件，使用冻结 DTO/适配器替身 |

`integration_dependencies` 只阻塞真实接线和联调，不阻塞上述独立开发；ODS-T03 与 ODS-T04 的双向联调关系不是开发依赖环。

ODS-T03 仅写网关配置后端文件，ODS-T04 仅写 LuCI 客户端，ODS-T05 仅写两个新 UI 子组件。已有 `AssetDeviceDialog.vue`、`AssetDeviceTable.vue`、页面 composable、ApiClient、共享类型、模块注册及路由，统一由 ODS-T01 定义骨架、ODS-T19 接线。并行作者不得顺手修改这些共享入口。

## 交接与完成标准

三任务向 ODS-T06/ODS-T10 交付可调用的配置与客户端，向 ODS-T19 交付配置区和入口事件。需满足 ODS-R01-A01～A05；配置可保存不代表真实路由器已连通，连接错误必须保留具体阶段。

后续验证覆盖角色/归属隔离、密钥不回传、空密钥保留、URL 与 TLS 错误、会话失效、四项 ACL 和页面无隐式扫描。**本轮仅拆分文档，未执行测试、构建或真实设备调用。**
