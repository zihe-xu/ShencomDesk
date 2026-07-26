# Rust Core 分层架构

ShenDesk Rust Core 使用以下依赖方向：

```text
Tauri Command
      ↓
Application Service
      ↓
Domain
      ↑
Infrastructure Adapter
```

## 目录职责

- `app`：启动、共享状态和生命周期。
- `commands`：接收 Tauri IPC 调用，只做输入输出适配。
- `application`：组织用例和服务流程。
- `domain`：领域类型与规则，不依赖 Tauri 或基础设施。
- `infrastructure`：数据库、缓存、文件、网络和系统适配器。
- `utils`：错误类型等横切工具。

## 约束

1. Command 不直接访问数据库、文件系统或网络。
2. Command 不实现业务规则。
3. Domain 不依赖 Tauri 和具体基础设施库。
4. Infrastructure 通过应用层定义的边界接入业务。
5. 应用启动资源由 `app::bootstrap` 统一注册。

## 基础验证链路

`health_check` 命令读取 Tauri 管理的运行状态，调用 `HealthService`，最终返回领域层的 `HealthStatus`。
