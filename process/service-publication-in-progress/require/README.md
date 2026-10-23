# 需求与接口契约

## 用户行为

1. 新增或编辑服务，选择“是否公开”。首次选择公开时打开配置弹窗。
2. 选择当前账号的公网服务器，再选择该服务器正在运行且可由 Web 修改的网络。
3. 传输协议沿用服务的 TCP/UDP；绑定地址默认 `0.0.0.0`，绑定端口、目标端口首次默认服务端口。
4. 目标地址读取设备当前组网 IP，只展示，不可修改。
5. 子弹窗确定仅更新草稿。点击服务保存时，原子保存服务与公网网络端口映射。
6. 保存为未公开时自动移除本服务的映射；编辑改绑时撤销旧映射再写入新映射。

## 请求和响应

沿用现有服务 POST/PUT，新增可选字段：

```ts
publication?: null | {
  machine_id: string;
  instance_id: string;
  bind_ip: string;
  bind_port: number;
  dst_port: number;
};
```

- 省略：兼容旧客户端，新建不公开，更新保持原公开配置。
- `null`：明确不公开，移除本服务拥有的映射。
- 对象：创建或更新公开配置。
- 不接受客户端提交目标 IP 或独立公开协议；从设备和服务推导。

服务响应增加 `publication`，未公开为 `null`；公开时包含上述五个字段及 `server_name`、`network_name`、`protocols`、`dst_ip`。

`GET /api/v1/assets/service-publication/networks?owner_user_id=...&machine_id=...` 返回：

```ts
{
  items: Array<{
    instance_id: string;
    display_name: string;
    network_name: string;
    selectable: boolean;
    unavailable_code: string | null;
    unavailable_reason: string | null;
  }>;
  total: number;
}
```

该接口只列实际运行中的网络。未保存、非 Web 配置、只读或中心托管实例展示不可选原因。仅返回展示信息，不返回网络密钥。公网服务器列表复用现有资产节点接口。

`unavailable_code` 用于中英文说明：`centrally_managed`、`config_not_saved`、`saved_config_not_web`、`saved_config_invalid`、`instance_id_mismatch`、`saved_config_stopped`、`runtime_metadata_missing`、`runtime_config_not_web`、`runtime_config_read_only`。

## 一致性与边界

- 服务、公开记录、目标网络 `port_forwards` 在同一个 SQLite 事务更新。
- 公网服务器必须属于所选账号；新公开或改绑必须重新核实运行状态及配置归属。
- 跟踪注入的精确规则及目标配置身份，只删除本服务拥有的规则。
- 绑定冲突返回错误并回滚，不能覆盖其他服务或手工规则。
- 删除服务、删除设备清理关联映射；设备地址变更同步更新映射目标地址。
- 配置完整且映射未变时，基本资料编辑允许公网节点离线。旧客户端省略公开字段并调整服务协议时，沿原完整绑定同步协议；新建、显式修改公开配置或恢复已被外部移除的映射需要新的运行状态证明。
- 设备地址自动同步前确认原自有规则仍在，避免因修改设备地址而恢复已被外部删除的公网入口。
- 挂靠公开服务的公网节点禁止直接删除，须先取消相关服务公开；检查先于停止网络或拉黑等副作用。
- 使用现有运行配置协调机制，保存成功表示配置已持久化，不表示已确认公网可访问。
- 离线撤销更新期望配置，实际运行端移除等待网络重新取得配置。
