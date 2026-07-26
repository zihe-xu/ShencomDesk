# ShenDesk Logging

ShenDesk 使用 `tracing`、`tracing-subscriber` 与 `tracing-appender` 构建本地日志系统。

## 日志目录

日志存放在 Tauri 应用数据目录下：

```text
<app-data>/logs/
├── app.log
├── error.log
└── operation.log
```

- `app.log`：应用启动、服务初始化和运行状态日志。
- `error.log`：仅记录 `ERROR` 级别事件，包括启动失败和未处理 panic。
- `operation.log`：记录关键业务或用户操作，使用 `shendesk::operation` target。

## 日志级别

默认日志级别为 `info`。开发环境可以通过 `RUST_LOG` 调整：

```bash
RUST_LOG=debug npm run tauri -- dev
```

## 记录普通日志

```rust
tracing::info!(project_id = %project_id, "project opened");
tracing::error!(error = %error, "project open failed");
```

## 记录操作日志

```rust
use crate::infrastructure::logging;

logging::record_operation("project.open", "success");
```

非阻塞 Writer 的 `WorkerGuard` 会作为 Tauri 托管状态保留到应用生命周期结束，以便正常退出时刷新缓冲日志。
