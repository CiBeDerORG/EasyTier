# ODS-R02 普通设备发现与逐条确认

状态：已确认，尚未实施。普通发现使用 LuCI `main@d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd` 的 `easy-discovery.devices.scan`；结果导入当前网关组。

## 已确认场景

用户点击“立即同步”，读取一次新发现结果并展示逐条差异清单。清单包含本次观测及来源保留的历史设备，展示来源时间、内网 IPv4、MAC、发现名称与已有记录差异。每条选择新建、更新、覆盖或跳过；正式应用前不改动已有资产。

先按当前网关组内网 IPv4 定位已有子设备。IPv4 未命中时，可手选同组非网关自身旧记录更新或覆盖；不能跨组挑选，也不能覆盖发起网关及其凭据。相同 IPv4 在组内唯一，跨组允许重复。

## 匹配与展示边界

- 首版以单 IPv4 为导入和匹配依据；IPv6 仅保存为发现资料，不参与归属、服务地址或自动化目标计算。源多 IPv4 不扩展为多设备或多服务，按公共契约留待处理。
- LuCI 的 `name` 可能是 MAC/IP 回退；有值的 `hostname` 才作为发现名称。未知 MAC、名称、型号不清空已有资料。用户显示名称与发现名称分别维护，避免人工改名后每次都重复冲突。
- 同 IP 不同 MAC、历史新旧记录复用同 IP或多条来源指向同一目标时，显示冲突，不按数组顺序自动覆盖。同组 IP 已占用时不能新增第二条同 IP 资产。
- 新建、更新和覆盖的候选目标、字段差异及预计地址均进入预览。无 IPv4 命中时，手选目标必须绑定本次预览，不能在提交阶段替换成未展示的其他设备。
- `recent`、`lease`、`cached` 和发现时间分别显示；租约、邻居缓存及 mDNS 不直接解释为实时在线。结果上限和部分失败必须可见。
- 失败、无快照、空结果不清空资产；返回旧快照时保留原时间，不能报告“本次发现成功”。可用部分结果仍须经过同一逐条选择流程。
- ADB 补采产生的普通预览绑定 `originating_operation_id`；应用结果交回原 ADB 操作继续处理，不能把提交普通预览当作再次扫描的授权。原 ADB 快照到期时需显式发起新发现。
- 普通同步不把既有手机降级为普通类型。覆盖的服务保留和正式事务属于 ODS-R03，不在预览阶段执行。

## 验收条件

| 编号 | 可观察结果 |
| --- | --- |
| ODS-R02-A01 | 主动扫描生成预览，历史条目、来源状态、原发现时间和截断提示均保留。 |
| ODS-R02-A02 | 按当前组 IPv4 初步匹配；跨组同 IP 不算冲突，同组重复不得创建两条。 |
| ODS-R02-A03 | 无 IPv4 命中可手选同组非网关自身设备；四种操作逐条保存，跨组/网关自身目标被拒绝。 |
| ODS-R02-A04 | MAC/发现名称变化与人工名称分别展示；缺字段不产生清空操作。 |
| ODS-R02-A05 | 同 IP 新旧 MAC 并存时明确呈现冲突，不自动取首条或自动删除历史。 |
| ODS-R02-A06 | IPv6 只记录；多 IPv4 不扩大首版范围；普通同步不会降级既有手机。 |
| ODS-R02-A07 | 失败、无快照、空列表不清空资产；预览过期或目标已变化时要求刷新差异。 |

## 实施入口与代码依据

- 后端预览：[PLAN-ODS-03](../plan/plan-ods-03-discovery-preview.md) / [ODS-T06](../task/task-ods-06-discovery-preview-api.md)。清单交互：[PLAN-ODS-05](../plan/plan-ods-05-device-sync-ui.md) / [ODS-T09](../task/task-ods-09-discovery-review-ui.md)。
- 正式应用交给 [ODS-R03](require-ods-03-asset-apply-overwrite.md)；拟新增预览字段及状态遵循 [公共契约 v1](../plan/contracts/v1.md)。
- LuCI 归一化依据：[root/usr/share/easy-discovery/discovery/format.uc](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/share/easy-discovery/discovery/format.uc) 的 `devices()`、`add()`、`findScope()`、历史快照合并逻辑；列表上限 256 条，历史可保留 24 小时。
- 当前应用约束：[easytier-web/src/db/assets/devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/assets/devices.rs) 的 `validate_relations()` 仍按节点限制 LAN IP 唯一；组级数据库约束由 ODS-T02 迁移，应用层 `validate_relations()` 及相关写入校验由 ODS-T07 修改，不能混淆所有权或误写成已具备。
