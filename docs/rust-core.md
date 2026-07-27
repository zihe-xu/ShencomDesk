# Rust Core 分层架构

ShenDesk Rust Core 使用依赖倒置后的调用与实现关系：

```text
Tauri Command
      ↓
Application Service ──定义──> Repository Port
      ↓                         ↑
Domain                  Infrastructure Adapter
                                ↓
                              SQLx
```

## 目录职责

- `app`：启动、共享状态和生命周期。
- `commands`：接收 Tauri IPC 调用，只做输入输出适配。
- `application`：组织用例、服务流程，定义持久化端口并提供进程内 EventBus。
- `domain`：领域类型、规则与事件协议，不依赖 Tauri 或基础设施。
- `infrastructure`：数据库、缓存、文件、网络和系统适配器。
- `utils`：错误类型等横切工具。

## 约束

1. Command 不直接执行 SQL、文件读写或网络请求。
2. Command 不实现业务规则。
3. Domain 不依赖 Tauri 和具体基础设施库。
4. Application 不导入 `infrastructure` 模块。
5. Infrastructure 实现 Application 定义的端口。
6. 应用启动资源由 `app::bootstrap` 统一注册。

## 配置存储示例

`application::config_repository::ConfigRepository` 定义配置键值的读取、写入与删除能力。

`ConfigService` 只依赖该端口，因此可以使用纯内存仓储测试默认值、迁移和损坏恢复。`DatabaseService` 位于 Infrastructure 层，通过 SQLx/SQLite 实现这个端口。

```text
ConfigService
      ↓
ConfigRepository
      ↑
DatabaseService (SQLx / SQLite)
```

## 基础验证链路

- `health_check` 命令读取 Tauri 管理的运行状态，调用 `HealthService`，返回领域层的 `HealthStatus`。
- 配置命令从 Tauri State 取得 `DatabaseService`，但只把它作为 `ConfigRepository` 传给 `ConfigService`，命令层不执行 SQL。

## 模块事件通信

模块间通知通过 `application::event_bus::EventBus` 发送 Domain 层 `AppEvent`：

```text
Publisher → EventBus (tokio broadcast) → Subscriber(s)
```

EventBus 支持独立多订阅者、按 `EventKind` 过滤、单调 sequence 以及慢订阅者 lag 检测。它只用于进程内通知；需要跨重启恢复的状态仍由 SQLite 持久化。详见 `docs/event-bus.md`。
