# ShenDesk Logging

ShenDesk 使用 `tracing`、`tracing-subscriber` 与 `tracing-appender` 构建本地日志系统。

## 日志目录与轮转

日志存放在 Tauri 应用数据目录下，并按 UTC 日期每日轮转：

```text
<app-data>/logs/
├── app.log.2026-07-26
├── error.log.2026-07-26
└── operation.log.2026-07-26
```

每类日志最多保留 15 个匹配文件，约等于当前文件加 14 天历史。创建新日志文件时，`RollingFileAppender` 会自动删除最旧文件。

- `app.log.*`：应用启动、服务初始化和运行状态日志。
- `error.log.*`：始终记录 `ERROR` 级别事件，包括启动失败和未处理 panic。
- `operation.log.*`：始终记录关键业务或用户操作，使用 `shendesk::operation` target。

## 日志级别与过滤

普通应用日志默认级别为 `info`。开发环境可以通过 `RUST_LOG` 调整：

```bash
RUST_LOG=debug npm run tauri -- dev
```

`RUST_LOG` 只控制 `app.log.*`。错误日志和操作日志使用独立 Layer 过滤器，不会因为 `RUST_LOG=off` 而被关闭。

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

## Panic 诊断

未处理 panic 会写入 `error.log.*`，字段包括：

- panic 文本；非字符串 payload 使用固定说明
- 源文件、行号和列号
- 当前线程名称

Panic Hook 不替换 Rust 默认处理逻辑；记录结构化信息后仍调用前一个 Hook。

## 正常退出

非阻塞 Writer 的 `WorkerGuard` 作为 Tauri 托管状态保留到应用生命周期结束。`RunEvent::Exit` 在数据库关闭和退出日志写入完成后主动释放 WorkerGuard，确保缓冲日志被刷新。
