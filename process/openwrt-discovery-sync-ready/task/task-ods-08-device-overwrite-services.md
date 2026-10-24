---
id: ODS-T08
feature: openwrt-discovery-sync
title: 覆盖重建与服务勾选保留
owner: null
status: todo
depends_on: [ODS-T07, ODS-T11]
integration_dependencies: [ODS-T17]
requirements: [ODS-R03, ODS-R08]
plan: PLAN-ODS-04
---

# ODS-T08 覆盖重建与服务勾选保留

所属：[ODS-R03](../require/require-ods-03-asset-apply-overwrite.md)、[ODS-R08](../require/require-ods-08-results-and-acceptance.md)；计划：[PLAN-ODS-04](../plan/plan-ods-04-device-apply.md)。待T07、T11交付后实施，经T07继承T15行为迁移前提；T17是旧目标清理的联调前提。当前负责人为 `null`。

## 输入与输出

输入为有效覆盖预览、同组旧设备、用户勾选保留的服务 ID 和对应版本。输出为新设备 ID、保留/删除服务清单、公开目标变化及旧目标失效描述；遵循[公共契约](../plan/contracts/v1.md)，不维护另一套迁移规则。

## 独占拟改范围

仅新增 `easytier-web/src/db/discovery/replacement.rs`，拟新增内部符号 `replace_device_in_transaction` 编排覆盖事务与保留关系，导出声明由T01冻结。只读T07设备事务适配、T11公开迁移辅助、实体外键和历史结构；不得改 `devices.rs`、`publications.rs`、迁移、会话/脚本模块或公共声明。

## 小步骤

1. 开启事务并重新核对组归属、非网关自身、目标 IP、旧设备/服务版本；勾选 ID 必须属于此设备，变化后要求重新确认。
2. 借助 T07 生成新设备记录与新 ID，在组内唯一约束及复合外键下完成关联转移，不能用现有 `delete_device` 直接级联清理。
3. 保留服务维持服务 ID、端口及内容；调用 T11 转移公开关系并更新目标。未保留项删除，仅清理其自有公开规则。全部关联迁移和旧记录删除在同一事务提交。
4. 历史运行/会话快照仍指向旧身份，不删不重写。未保留 ADB 就不产生新的自动化主机来源，也不从旧连接配置补回。提交后交接旧目标失效，不能在数据库事务内重连。

## 完成标准与未来验证

满足 ODS-R03-A05～A08。未来验证全保留、全不保留、部分公开服务、ADB保留分支、陈旧勾选、关联迁移失败回滚与重复提交。整任务连带取消由T15唯一修正；T02不修改该行为。旧会话退出未确认前，不得把新目标投入旧脚本。本轮测试、构建、CI均未执行。

## 交接

向 [T09](task-ods-09-discovery-review-ui.md)/T19提供新旧ID与逐项结果；向 [T11](task-ods-11-public-mapping-apply.md)交付提交后的应用描述，向T17交付旧目标失效事件。旧执行历史和新连接身份分开，实际运行应用与重连结果由对应任务反馈。
