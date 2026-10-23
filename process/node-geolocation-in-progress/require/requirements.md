# 节点地理定位需求

## 用户目标

网络节点基本属性支持设置国家、城市、地区、经纬度。允许通过免费地图选点，并自动生成国家、城市和地区，作为后续网络拓扑地图大屏的基础数据。

## 功能范围

1. 普通网络节点和节点资产的属性编辑使用同一套位置字段与地图交互。
2. 保存国家 `country`、省州/地区 `region`、城市 `city`、区县 `district`、纬度 `latitude`、经度 `longitude`，统一采用 WGS84。
3. 点击地图选点后自动解析行政区信息；行政区字段可手动修正，也可完全手动录入。
4. 允许只填写行政区信息；经纬度必须同时为空或同时提供有限数，纬度范围 `[-90, 90]`，经度范围 `[-180, 180]`。零坐标是有效值。
5. 地图和解析只修改表单草稿；保存沿用现有节点属性接口，取消不保存。打开地图不得自动把默认视野中心作为节点位置。
6. 清除定位会清空该节点的手工地理位置。心跳、重连及 IP 归属地更新不覆盖手工地理位置。
7. 底图和地址解析不可用时保留已有草稿，允许手动保存。更换坐标、手动修正地址、关闭表单后，迟到解析结果不能覆盖当前输入。
8. 两套节点查询返回相同的结构化位置，供后续拓扑地图使用。本轮不实现大屏或地理位置搜索。

解析防抖和请求进行中暂缓提交，以便自动回填与所选坐标一起保存；成功、失败、超时或用户改为手动输入后恢复保存。解析无结果或失败时保留已有文字，并提示用户核对后手动填写。

## 数据契约

`geo_location` 为独立于 `site_location` 和 `ip_*` 的嵌套对象：

```json
{
  "country": "中国",
  "region": "北京市",
  "city": "北京市",
  "district": "东城区",
  "latitude": 39.9042,
  "longitude": 116.4074,
  "coordinate_system": "WGS84"
}
```

示例仅说明结构；地址解析结果由地图数据决定，用户可修正，不能保证行政区信息完整。

| 输入形式 | 保存语义 |
| --- | --- |
| 省略 `geo_location` | 保留原定位，兼容旧客户端 |
| `geo_location: null` | 清空原定位 |
| `geo_location: { ... }` | 整体校验后替换；全空对象规范为清空 |

四个行政区文本字段最多 128 个字符，去除首尾空白后空字符串保存为 `null`。经纬度均允许 `null`，但必须成对。`coordinate_system` 输入可省略；提供时仅接受 `WGS84`，读取时固定返回 `WGS84`。不把所选位置替换成地址服务返回的附近地物中心。

## 接口范围与权限

| 接口 | 本轮行为 |
| --- | --- |
| `GET /api/v1/assets/nodes` | 在当前账号作用域中返回 `geo_location` |
| `PATCH /api/v1/assets/nodes/{machine_id}/metadata` | 与原属性同事务保存定位；保留账号本人或管理员权限 |
| `GET /api/v1/machines` | 返回同一定位结构 |
| `PUT /api/v1/machines/{machine_id}/attributes` | 保存当前账号节点的定位；旧请求省略字段时保留定位 |
| `GET /api/v1/geocoding/config` | 登录后读取可用底图配置和解析开关 |
| `GET /api/v1/geocoding/reverse` | 登录后按显式坐标执行解析，仅返回行政区字段，不写入节点 |

外部 webhook 模式沿用原有节点手工属性写入限制。清空、删除、重新保存均须遵循原账号作用域。

## 地图服务约束

- 默认使用 Leaflet 1.9.4、OpenStreetMap 标准底图、Photon 逆地理解析，无需终端用户填写地图密钥。
- 底图保留可见署名，按用户打开、缩放和平移加载；不批量预取，不后台轮询底图或地址。
- 解析仅在用户选点或主动请求时发生。前端防抖及请求序号隔离；后端复用客户端、全局限速、有界缓存、超时和响应体大小限制。
- 上游只允许服务端配置，前端不能提交任意请求 URL。地图不可用时可继续手动录入。
- 公共服务受其使用政策与可用性限制，部署者可替换底图及 Photon 兼容解析端点。

## 官方依据

- [Leaflet 稳定版本](https://leafletjs.com/download.html)
- [Leaflet API](https://leafletjs.com/reference.html)
- [OpenStreetMap 瓦片使用政策](https://operations.osmfoundation.org/policies/tiles/)
- [Photon 项目与公共服务说明](https://github.com/komoot/photon)
- [Photon 解析 API](https://github.com/komoot/photon/blob/master/docs/api-v1.md)
