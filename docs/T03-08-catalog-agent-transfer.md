# T03-08 — Deploy 固定版本包 → 配对 Agent 安全交付

> EasyTier 分支：`feat/server-app-lifecycle`，2026-10-29。关联 [Agent 端交付验收](https://github.com/CiBeDerORG/EasyHostAgent/blob/feat/server-app-lifecycle/docs/T03-08-agent-package-verification.md)。

## 完成的控制端代码

- `easytier-web/src/deploy_catalog.rs`：读取 T03-07 固定 commit 生成的 tar，并在实际交付前再次核对文件大小及 SHA-256；返回固定 `app_id/git_revision/manifest_sha256/package_sha256`，不使用移动的 Git main。
- `easytier-web/src/asset_agent.rs`：直接复用已有 Agent 设备绑定、按 owner 隔离的加密 Bearer、已配对 Controller ID，以及 TLS 1.3 **证书 SPKI SHA-256 Pin**；不从请求参数接受任意 IP/URL。
- `easytier-web/src/asset_agent_tls.rs`：为上传使用 90 秒有界超时，保留既有 HTTPS-only、无代理、无重定向、连接前 pin 验证逻辑。
- `easytier-web/src/restful/deploy_catalog.rs` 新增 API：
  `POST /api/v1/deploy/catalog/{app_id}/transfer/{device_id}?owner_user_id=N&machine_id=...`
  **无请求体、无密码字段**；先复用服务器设备的 `load_authorized` 归属/语义校验，再对存在且已配对的 Agent 执行上传。校验 Agent 响应的 `X-EHA-Agent-ID`、应用 ID、Git Revision、双摘要和 `install_enabled=false`。
- Agent 受控上传接口：`POST /v1/deploy/packages`，使用现有配对 Bearer，`Content-Type: application/x-tar` 与 `X-EHA-Package-SHA256`，不使用独立 Deploy Token。
- 控制端页面的安装按钮仍禁用，**分发只表示数据已保存在 Agent**，不得推断 Docker 已安装或应用已接管。

## 责任分界

| 层 | 负责 | 禁止 |
|---|---|---|
| Deploy | 维护 app.yaml、脚本与 Compose 原始来源 | 接收主机 Bearer Token |
| EasyTier | 人工 Git 更新，固定 commit 解析打包，检查用户与服务器归属，安全发送 | 让浏览器传任意 Git URL/Agent 地址，自动安装 |
| EasyHostAgent | 验证摘要/动作/路径/大小/归档内容，0600 不可变保存，返回回执 | 自行拉 Git、执行尚未授权的 Shell / Docker |

## 待验收

- [x] Go Agent 专用包验证回归、双控制端隔离、Go race 双架构 CI **通过**：[#40809855033](https://github.com/CiBeDerORG/EasyHostAgent/actions/runs/40809855033)。
- [x] EasyTier 新分支 `node-attributes-image.yml` CI 已加入 `feat/server-app-lifecycle` 与独立 `feat-server-app-lifecycle` 镜像标签。
- [ ] EasyTier 新分支 CI **尚未确认最终通过**：[查看构建 #40810348754](https://github.com/CiBeDerORG/EasyTier/actions/runs/40810348754)。
- [ ] 真实 Linux/FnOS 服务器连通、只读 Git 凭证、Portainer/Sub2API 包传输、重连、离线重试与已下架包保留实测。

**本项不包含 M4 任务队列、解压和执行。** 下一轮先检查 CI 失败并修复，再实现 T04-01 持久化任务状态机；不启用自动回滚。
