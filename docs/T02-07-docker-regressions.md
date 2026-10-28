# T02-07 — Docker 发现回归测试与真实 FnOS 阻塞

> 2026-10-28，`CiBeDerORG/EasyTier@feat/automation-center` + `CiBeDerORG/EasyHostAgent@main`。
> 状态：**自动化回归代码已提交；真实 FnOS/用户界面验收需现场授权，不标记整项 DONE。**

## 已提交的模拟回归

### EasyHostAgent Go — 真实 Unix socket 模拟 Engine

新文件 `internal/dockerengine/discovery_regression_test.go`，基于现有 `testEngine`：

- 同时识别 Portainer 停止项目、Sub2API Web + PostgreSQL + Redis 三容器项目、两个使用相同镜像的独立容器。
- 5 次读取完整清单，故意变更 Docker Engine 返回次序，检查 Stable Resource ID、Compose Service 排序、状态汇总和无重复项目。
- 一个容器停止或整个 Compose 项目停止，仍包含在 `all=1` 清单里，不能误触发外部删除。
- Engine containers 成功但 images 列表返回 503，整体必须 `ErrIncomplete` 且清空不可权威分组；恢复后资源身份不变。
- 两个 Docker Unix Socket 有同名 Compose，前者与后者的容器 ID 不同，数据隔离由每个 Host 的独立 Client 完成。
- [Go 构建及 race 测试全部成功 #40666918325](https://github.com/CiBeDerORG/EasyHostAgent/actions/runs/40666918325)。

### EasyTier Rust — 本机 SQLite 真迁移数据

新文件 `easytier-web/src/docker_discovery_regression.rs`，以 `Db::memory_db()` 执行真实 migration：

- 两个完整 Compose（单容器和三成员）、一个停止项目和一个停止的独立容器，4 次成功重复扫描不会产生重复快照/缺失候选。
- 人工创建的 EasyTier 服务不会随两轮确认缺失的 Docker 项目被删除。
- 非完整、错误状态、Agent UUID 不一致、重复资源 ID 都不能改变最后成功的快照和缺失证据。
- 两个不同用户/服务器上同名 `compose:sub2api` 的快照、Agent UUID 与接管意向完全隔离。
- SQLite 主键和唯一键拒绝同一主机重复资源或应用的冲突接管意向。
- 修改现有 `.github/scripts/test-native.sh`：将 `docker_absence` 和 `docker_discovery_regression` 加入 Native CI 必须执行且至少有一条成功用例的名单，避免镜像构建通过而新测试未实际运行。
- **最新 Rust CI 待检查**；本轮不等待构建。

## 业务边界仍需后续收尾

- T02-04 可信 Deploy manifest 匹配、Agent 受管所有权确认依赖 T03/M4。
- T02-06 真实删除服务及公网转发需要 M4/M5 的排他归属及发布系统同步；当前仅为缺失证据，`cleanup_allowed=false`。
- T02-05 离线历史 UI 已通过构建，但真实 FnOS/网络故障验收仍未执行。
- FnOS 真机验收矩阵：参照 [EasyHostAgent T02-07 FnOS 清单](https://github.com/CiBeDerORG/EasyHostAgent/blob/main/docs/T02-07-fnos-docker-discovery-checklist.md)，F01～F15；当前无主机连接，现场验收仍 **BLOCKED**。

## 下一任务

按全局规则先检查 EasyTier 新 CI，失败修复、运行中不等待。自动回归验收后可以进入 **T03-01 Deploy app.yaml v1 解析校验器**，继续补齐 T02-04 接管权限的可信商店前置条件。
