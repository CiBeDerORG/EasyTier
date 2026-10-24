# 行政区域数据

`administrative-boundaries.json` 是随前端打包的真实行政区参考数据。国家/地区边界与名称用于地球远景，一级行政区（省、州等）在放大后显示；近景可显示视线落点所在行政区。全部从本地资源读取，不依赖地图账号、瓦片服务或用户浏览时的第三方网络请求。

## 来源与许可

- 来源：[Natural Earth](https://github.com/nvkelso/natural-earth-vector)。数据采用其默认边界表达及原有中英文名称；争议/未确定边界类别保留并使用虚线绘制。
- 固定源码版本：[`ca96624a56bd078437bca8184e78163e5039ad19`](https://github.com/nvkelso/natural-earth-vector/tree/ca96624a56bd078437bca8184e78163e5039ad19)。取得和整理日期：2026-10-24 UTC。
- 许可：[Public domain](https://github.com/nvkelso/natural-earth-vector/blob/ca96624a56bd078437bca8184e78163e5039ad19/LICENSE.md)，允许修改和再分发。
- 国家/地区：1:50,000,000 的 Admin-0 country polygons 与 land boundary lines，共 242 个国家/地区要素。
- 一级行政区：1:10,000,000 的 Admin-1 states/provinces polygons 与 boundary lines，共 4,596 个要素；覆盖全球，包括中国省级区域。

这是固定版本的地理参考数据，并非实时行政调整接口、法律界址或街道地图。缩放时实时更新的是显示层级、标签与当前视域，不代表边界数据实时更新。原始比例尺及下述简化会造成局部偏差，不宜用来判定一个地址或设备的法定行政归属。市、县、街道层级未包含，也不会由省级数据推测生成。

## 处理方式

生成脚本：[build-administrative-map.py](../../../scripts/build-administrative-map.py)。脚本只读取已经下载的四个固定来源文件，不进行网络请求。它核对 SHA-256 后进行以下数据整理：

1. 仅保留边界线、中英文名、标签锚点、所属国家/地区以及用于视点归属的行政面，移除其他统计与来源属性。
2. 经度跨越日期变更线时先连续展开，然后执行确定性的 Ramer–Douglas–Peucker 简化。国家边界线容差为 `0.012°`，一级边界线为 `0.006°`；行政面分别为 `0.015°`、`0.008°`。经度距离按所在纬度缩放，避免高纬度过度简化。
3. 行政面保留多面及洞，不用最近地名猜测视点归属。原始空几何不生成线，较小的面若简化后不足闭合环则保留原始环。
4. 坐标保留到 `0.0001°`，以 `10,000` 比例整数及差分数组储存。这个精度只是文件编码精度，不提高源数据的地图准确性。

成品为 7,300,748 字节，gzip 参考大小约 3,006,635 字节；SHA-256 为 `37397ce996346009fc9ee96484d765d32c0f6ff9ec9da7db2bbfa83fe1b94796`。数据仅在异步加载地球组件时进入前端模块，不在仪表盘初始化时加载。

运行时将长线分段投影至球面，防止近景弦线被地球遮住。省界按地理网格分块，可由相机视锥裁剪；远景隐藏省界。地名先过滤背面、视野与不足容纳标签的区域，再避让网络节点标签；最多显示 18 个国家/地区标签、28 个一级行政区标签和一个近景区域上下文。相机与节点标签位置不变时复用结果，流量动画不会每帧重新进行行政面查询。

## 更新与复现

更新时先审阅 Natural Earth 的新数据、名称、几何与许可，再更改脚本内固定版本和四个来源哈希；不能仅替换下载 URL 或跳过哈希核对。

现有版本可用以下命令重新取得输入并整理：

```sh
mkdir -p /tmp/easytier-natural-earth
for name in ne_50m_admin_0_countries ne_50m_admin_0_boundary_lines_land ne_10m_admin_1_states_provinces ne_10m_admin_1_states_provinces_lines; do
  curl --fail --location "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/ca96624a56bd078437bca8184e78163e5039ad19/geojson/$name.geojson" -o "/tmp/easytier-natural-earth/$name.geojson"
done
python3 easytier-web/frontend/scripts/build-administrative-map.py /tmp/easytier-natural-earth
```

原始文件 SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| `ne_50m_admin_0_countries.geojson` | `3e458fc036ad0a66411f2c1e6cac49c5d7bfb81cb1123bc513b22511a2b7fdeb` |
| `ne_50m_admin_0_boundary_lines_land.geojson` | `2faac4f6b34386f3d21b6e018cf151f241f00e5c936d44dd17d7d9bfb147fa48` |
| `ne_10m_admin_1_states_provinces.geojson` | `22d0e3ad85eb3e27f17cabf8ba2d50e554fbc27a87796ff891d958185da62fb5` |
| `ne_10m_admin_1_states_provinces_lines.geojson` | `1a1f30ccaaf4cc9c4bde34266f0b8cbb955d3a4cf254b756912255f2ec7c75b6` |
