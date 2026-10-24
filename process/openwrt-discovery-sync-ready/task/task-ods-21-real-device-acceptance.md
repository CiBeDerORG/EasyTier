---
id: ODS-T21
title: 真实 OpenWrt、手机与公开映射验收
status: todo
owner: null
requirements: [ODS-R01, ODS-R02, ODS-R03, ODS-R04, ODS-R05, ODS-R06, ODS-R07, ODS-R08]
plan: PLAN-ODS-10
depends_on: [ODS-T20]
integration_dependencies: []
---

# ODS-T21 真实 OpenWrt、手机与公开映射验收

- 所属计划：[PLAN-ODS-10 接线、验证与交付](../plan/plan-ods-10-integration-and-delivery.md)。
- 需求：[R01–R08 索引](../require/README.md)；具体验收按本任务范围与[公共契约](../plan/contracts/v1.md)关联。
- 状态：未认领、未开始；文档编写不代表实施。

## 目标与边界

在真实OpenWrt、Android手机、EasyTier运行网络和公开服务器上验收用户操作闭环。硬依赖T20；需要用户授权的测试范围与实际可达环境，不能把当前未连接的路由器/手机当作已经验证。

## 输入输出与写入

输入待验收提交、LuCI基线/权限、受信HTTPS访问方式、运行网关组代理规则、已授权手机及公开映射环境。只在本需求 `task/artifacts/real-device/` 和验证记录写脱敏结果，不保存密码/配对码。配置或业务缺陷按所有权交回，不扩展无关设备扫描。

## 小步骤

验证网关自身授权与两入口；发现历史、组内冲突/跨组同IP、手选旧设备换IP；分别覆盖保留全部/部分/无服务并检查公网规则。验证ADB缺设备仅补一次后跳过；唯一服务端口变化与原公网入口不变、真实目标更新；同时让两台手机参与脚本，改变其中一台端口，确认它退出而另一台继续，再确认原投屏恢复且旧动作不重放。记录未配对、公开apply失败、旧清理未确认、页面离开后的真实提示与重试结果。

## 完成与交接

按各需求验收ID留可核对证据、环境、提交和时间；无法获得环境的项明确未执行。交T22已通过范围及限制，不能将需求ready或模拟结果作为真实验收。当前本任务todo，未使用实际设备、未执行测试。
