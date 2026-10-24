# ODS-R01 网关发现接入与授权

状态：已确认，尚未实施。源码基线为 EasyTier `dbbade81d8edfd51bc3844c393fb9afc650b51d1`；LuCI 为 `d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd`。

## 已确认场景

用户在当前网关组的“网关自身”资产设备基本信息中，将设备类型设为 OpenWrt，填写发现接口地址和授权密码。设备行提供“立即同步”和“ADB 发现”入口，操作始终使用该设备所属网关组，不扩大到网络节点的其他组。

默认接口地址为 `https://<网关组网IPv4>:443/ubus`，允许修改。默认值只用于尚未配置的地址，后续编辑不覆盖已保存的自定义地址。用户名复用 LuCI 专用账号 `easyadb`，不是路由器管理员账号。

## 业务边界

- 发起设备同时满足 `is_gateway_self=true`、具备 OpenWrt 类型语义、属于当前网关组；后端重新检查归属和权限，不能只依赖前端按钮显示。
- 地址及密钥按网关自身资产设备保存。读取接口仅返回配置状态和脱敏信息；密钥不进入普通资产响应、日志、URL 或发现结果。未填写新密钥时保留原密钥，清除必须使用明确的清除意图，清除凭据不删除已有资产。
- 保存配置和发现请求相互独立。打开基本信息、读取设备列表、读取快照均不发起扫描；“检查接入”只验证登录与固定能力，不触发普通或 ADB 扫描。
- LuCI 不提供独立发现端口，也不返回自身 Web 服务的监听配置；现有组网 IPv4 只能作为主机默认值。TLS 连接失败应返回可识别错误，不能悄悄关闭证书校验或自动降级 HTTP。
- 客户端执行 RPCD 登录及授权范围内的 `list`、`scan`，不安装软件、控制服务或管理路由器授权。旧兼容扫描权限不等于新的四项发现权限。
- 本需求只准备配置和入口；普通同步清单属于 ODS-R02，ADB 结果属于 ODS-R04，真实路由器连通性留待后续授权验证。

## 验收条件

| 编号 | 可观察结果 |
| --- | --- |
| ODS-R01-A01 | 合格网关自身设备显示两个发现入口；普通设备、其他类型和越权请求被拒绝。 |
| ODS-R01-A02 | 默认地址正确，地址可改；空密钥编辑保留原值，读取响应不返回明文密钥。 |
| ODS-R01-A03 | 请求固定登录 `easyadb`；登录失败、权限不足、会话失效、连接或证书错误可区分。 |
| ODS-R01-A04 | 保存配置、打开页面和读取列表不会扫描；用户明确点击才发起相应发现。 |
| ODS-R01-A05 | 操作上下文包含网关自身设备、当前网关组和所属用户/节点；切换组或设备后不会误用旧凭据。 |

## 实施入口与代码依据

- [PLAN-ODS-02 网关授权设置与 LuCI 接入](../plan/plan-ods-02-gateway-access.md)：ODS-T03、ODS-T04、ODS-T05。
- 持久化及共享接口依赖 [PLAN-ODS-01](../plan/plan-ods-01-contracts-and-storage.md) 与 [公共契约 v1](../plan/contracts/v1.md)；所有新增接口与组件均为拟新增。
- 当前资产字段：[easytier-web/src/db/entity/asset_devices.rs](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/src/db/entity/asset_devices.rs)，已有组归属、类型、组网地址和网关自身标记，尚无发现凭据字段。
- 当前基本信息界面：[easytier-web/frontend/src/components/node-assets/AssetDeviceDialog.vue](https://github.com/CiBeDerORG/EasyTier/blob/dbbade81d8edfd51bc3844c393fb9afc650b51d1/easytier-web/frontend/src/components/node-assets/AssetDeviceDialog.vue)。共享界面的接线由 ODS-T19 负责。
- 认证与方法：[LuCI docs/API.md](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/docs/API.md)、[docs/AUTHENTICATION.md](https://github.com/CiBeDerORG/luci-app-easy-discovery/blob/d3dbe4f2ef823b05b8fe5a9415aa274ecede9cdd/docs/AUTHENTICATION.md)。
