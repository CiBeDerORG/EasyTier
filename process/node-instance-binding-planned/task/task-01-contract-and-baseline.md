---
id: NIB-T01
plan: P01
status: completed
owner: contract-owner
depends_on: []
updated_at: 2026-10-23
---

# T01 冻结基线、DTO与组件契约

已冻结v2实施契约，第9节包含DDL、精确DTO、helper签名与前后端交接约定；依赖模块按该版本实施。

## 输入与源码依据

- 本轮require-01至04、`plan/contracts/v2.md`与上一轮v1。
- `easytier-web/src/db/entity/{asset_gateway_groups,asset_devices,user_running_network_configs}.rs`：资产无绑定、配置有行身份。
- `easytier-web/src/restful/assets/mod.rs`及`frontend/src/types/node-assets.ts`：授权作用域、已有REST及客户端形状。
- `easytier-core/src/management/full/remote_client.rs`、`config/api_input.rs`：旧在线目录、配置读取及proxy语义。

## 实施内容与输出

1. 基于v2草案冻结增量字段和索引、legacy回填、作用域和配置世代；明确所有可空值对新旧记录的不同规则。
2. 固定候选、预览、attach/refresh、地址修正、普通CRUD DTO和错误码；给direct/gateway/child/self各一套请求响应示例，以及changed/replaced/legacy例子。
3. 固定T03读取快照、T04纯推导、T06状态比较、T05事务写入的调用签名和依赖顺序；普通/中央配置更新只读发现。
4. 固定T08客户端及T09/T10/T11 props/events；网关创建和普通设备创建后刷新路径分别明确。
5. 核对代码落点与看板唯一owner；把工程默认标为本轮方案，不伪称来自用户答复。留实施细节给owner处理，不留下“后端自行决定，前端猜测”的空洞。

## 独占与交接

独占本轮契约、汇总及owner矩阵；不修改实现源码。交接T02/T03/T04/T06/T08：契约版本、DDL、接口样例、状态动作、错误映射、共享文件归属。其他agent发现契约问题回报本owner统一改。

## 完成条件

- 前端每个入口都有对应API，API的成功/错误/失效分支都有页面动作。
- 新增、补绑、核对、改地址、删除和服务操作均能按契约描述完整往返。
- 网关/直属分类、网关自身地址、普通代理、映射、非/24、旧IPv6、同ID重建都有明确处理。
- v2从draft_for_t01改为frozen，依赖任务可领取；这里只人工审阅文档，不执行测试、构建或CI。

## 状态

`completed`：契约已冻结并交给各模块owner，运行结果待T13授权验证。本任务完成不代表整项功能已实现。
