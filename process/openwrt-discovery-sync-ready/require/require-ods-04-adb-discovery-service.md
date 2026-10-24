# ODS-R04 ADB 发现与唯一服务维护

状态：已确认，尚未实施。依赖已授权的网关接入、普通发现及资产应用能力；不从旧自动化连接端口推导或补建服务。

## 已确认场景

用户在 OpenWrt 网关自身设备点击“ADB 发现”，通过 `easy-discovery.adb.scan` 获取连接服务。按当前网关组单 IPv4 匹配资产，匹配成功后将设备标识为手机：已有唯一 ADB 服务时更新端口，没有时新建该服务。

存在未匹配端点时，整次操作最多补一次普通发现。补采只处理这些未匹配 ADB IPv4 对应的普通记录，不批量导入其他设备；沿用普通设备的导入和冲突规则，再尝试匹配一次。仍无对应设备则跳过，不仅凭 ADB 地址创建缺少普通发现依据的设备。冲突记录留待选择，不反复扫描或任意覆盖。

需要人工选择的普通预览保存 `originating_operation_id`；原操作保留 ADB 快照、`pending_ips`、已使用补采标志及到期时间。`devices/apply` 后，由 ODS-T07 返回逐行结果，ODS-T19 交给 ODS-T10 继续原操作；跳过也会结束对应待处理项。续接不得再次执行 ADB 扫描或第二次普通补采，原快照过期则显示需要重新发现，由用户显式新建扫描操作。

## 服务与失败边界

- 只接受 `_adb-tls-connect._tcp` 连接服务；配对服务不是连接端口。发现不代表已配对、已连接或可投屏。
- 一手机一条结构化 ADB 服务，一条服务一个端口。用稳定服务语义识别，不能仅靠显示名称、图标或旧 `automation_connections` 端口判断；缺少结构化类型的旧服务进入待确认状态，不随意猜测。
- 同一 IPv4 返回多个候选连接端口时不能随机取首条；使用原操作的 `resolve` 选择保存快照中的 `selected_endpoint_id`，不能由客户端另传任意端口。IPv6 仅记录，不参与本期服务匹配。
- 更新现有 ADB 服务保留服务 ID 与公开关系，不重复新建。端口被其他服务占用时明确报告冲突，不删除其他服务；首次建立 ADB 服务交给后续协调器尝试连接。
- 已公开服务保留公网入口，更新目标地址、端口并应用到运行网络，执行责任属于 ODS-R05。端口变化后立即重连、原投屏恢复及单设备退出脚本属于 ODS-R07。
- 只有资产和服务事务成功提交后才发布后续目标变化事件。手工修改 ADB 服务端口与发现更新使用同一事件链路；目标未变不重连。事务失败不能先触发映射或重连，相同请求重放不重复建服务或反复重连。
- ADB 的 `error`、`busy`、`has_snapshot=false` 及失败来源携带的旧端点仅供展示：禁止据此改手机类型、覆盖端口、修改公开映射或触发重连。`partial` 只处理可明确证明来自本次成功来源的有效端点，不能直接复用普通发现允许历史导入的规则。
- 扫描失败、无快照、空结果不删除已有 ADB 服务。已保存、公开映射应用、ADB 连接分别报告；未授权或连接失败保留已同步信息，可重试失败阶段。
- 操作 `retry` 只允许 `publication`、`connection`、`cleanup`，绑定原事件及当前端点版本；不重新扫描，不重放成功阶段或旧脚本。扫描失败本身需要用户显式再次发现。

## 验收条件

| 编号 | 可观察结果 |
| --- | --- |
| ODS-R04-A01 | 仅处理 IPv4 连接服务；配对与 IPv6 记录不会成为当前 ADB 连接端口。 |
| ODS-R04-A02 | 匹配范围为当前网关组；整次最多补一次普通发现，仅补未匹配 IP 对应记录，仍未匹配则逐项跳过。 |
| ODS-R04-A03 | 成功匹配改为手机，并创建或更新唯一 ADB 服务；重复执行不产生第二条服务。 |
| ODS-R04-A04 | 多候选端口、身份冲突或无可用地址时显示待处理原因，不任意选取。 |
| ODS-R04-A05 | 不使用旧自动化端口兜底，也不据旧连接记录反向生成服务；非 ADB 服务保持原样。 |
| ODS-R04-A06 | 提交后产生一次可去重的目标变化交接；公开入口保留，后续映射/重连状态单独展示。 |
| ODS-R04-A07 | 失败和空结果不清空已有服务；保存成功但映射或连接失败时不会伪报全部成功。 |
| ODS-R04-A08 | 补采预览应用后沿原操作快照续接，保留已补采标志；跳过可结束待处理项，不再扫描；过期必须显式新发现。 |
| ODS-R04-A09 | ADB 失败、忙碌、无快照及失败附带旧数组不会改类型、端口、公开映射或触发重连；部分成功仅写本次可证明有效端点。 |
| ODS-R04-A10 | 端口歧义只能选择原快照端点；重试仅执行指定失败阶段，旧版本、任意新端口或越界事件被拒绝。 |

## 实施入口与代码依据

- [PLAN-ODS-06](../plan/plan-ods-06-adb-single-service.md)：[ODS-T10](../task/task-ods-10-adb-reconcile.md)、[ODS-T12](../task/task-ods-12-adb-discovery-ui.md)。
- 后续联动：[ODS-R05](require-ods-05-public-mapping.md)、[ODS-R07](require-ods-07-reconnect-script-exit.md)。事件和拟新增服务类型遵循 [公共契约 v1](../plan/contracts/v1.md)。
- LuCI 实际扫描：[root/usr/libexec/luci-easy-discovery-adb](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/libexec/luci-easy-discovery-adb)，内部 Avahi 窗口为 8 秒。
- 端点格式与去重：[root/usr/share/easy-discovery/discovery/format.uc](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/root/usr/share/easy-discovery/discovery/format.uc) 的 `browseRecord()`、`adb()`；当前最多 32 个端点，端点没有 MAC。
- 当前资产/服务模型：[easytier-web/frontend/src/types/node-assets.ts](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/types/node-assets.ts) 的 `AssetDevice`、`AssetService`；结构化 ADB 类型与唯一约束仍为拟新增。
