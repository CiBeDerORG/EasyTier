---
id: NIB-P01
status: completed
tasks: [T01]
updated_at: 2026-10-23
---

# P01 基线和跨端契约

**最新状态：** 本计划已完成对应开发、授权验证及当前云环境交付；实际结果和未覆盖项见[验证报告](../task/verification-report.md)与[交付记录](../task/release-record.md)。以下验收设计保留，不将未实测范围扩大为已通过。

将用户确认的模型与本轮工程默认落实到同一版API/存储/组件协议，再并行实施，避免角色、实例和组网IP在不同端各自解释。

依据：[范围与改动矩阵](../require/require-01-scope-and-change-matrix.md)、[v2草案](contracts/v2.md)。当前asset模型没有绑定、旧实例目录要求在线、中央跨节点复用instance_id，均已有源码依据；不重复访谈用户。

T01独占契约，确定保留groups/devices的增量DDL、离线持久配置目录、source/世代/指纹、地址判别联合、状态动作和错误，以及前端组件props/events。确认旧资料补绑和同实例核对可从页面走到事务及刷新；不要只写创建成功路径。

交付是冻结契约、样例请求/响应与文件owner矩阵。完成后T02/T03/T04/T08可并行；T06生命周期模块也可按纯数据输入先行。文档审核不等于代码验证，不运行测试/CI。
