# ShenDesk 配置系统

## 默认配置

```json
{
  "schemaVersion": 1,
  "theme": "system",
  "language": "zh-CN",
  "autoStart": true
}
```

## 认证环境变量

认证服务使用仓库根目录的模式文件：

| 模式 | 配置文件 | 凭据库账户 |
|---|---|---|
| `development` | `.env.development` | `development-session-v1` |
| `production` | `.env.production` | `production-session-v1` |
| `test` | `.env.test` | `test-session-v1` |

每个模式文件必须提供 `HOST` 和 `SCID`。`pnpm tauri -- dev` 默认加载 `development`，`pnpm tauri -- build` 默认加载 `production`；进程变量 `SHENDESK_AUTH_ENVIRONMENT` 可以显式选择 `development`、`production` 或 `test`。`.env.local` 按 Vite 规则提供本地覆盖。

Tauri 启动器只读取 `HOST`、`SCID` 和所选模式，并映射为 Rust 专用的 `SHENDESK_AUTH_HOST`、`SHENDESK_AUTH_SCID`、`SHENDESK_AUTH_ENVIRONMENT`，不会把测试账号或其他环境变量注入 WebView。专用变量避免与 Cargo 的 `HOST` 目标三元组变量冲突；生产构建通过 `build.rs` 嵌入同一配置，使安装后的应用不依赖仓库中的环境文件。缺少配置、非 HTTPS `HOST`、空 `SCID` 或不支持的模式会阻止认证服务初始化，不会回退到其他环境；不同模式不会复用系统凭据库中的 Token。

## 存储

配置以 JSON 形式存储在 SQLite 的 `app_config` 表中，键名为 `app.settings`。数据库文件位于 Tauri 应用数据目录的 `app.sqlite`。

## 生命周期

1. 应用启动时创建应用数据目录和 SQLite 数据库。
2. SQLx 自动执行 `src/infrastructure/database/migrations` 中的迁移。
3. `ConfigService::load` 通过 `ConfigRepository` 读取 `app.settings`。
4. 配置缺失时写入默认值。
5. 旧配置会升级到当前 `schemaVersion`，升级结果会自动回写。
6. 不支持的主题和空语言值会被归一化。
7. 配置 JSON 损坏时执行恢复流程，应用不会因为解析错误而停止启动。

`theme` 支持 `system`、`light` 和 `dark`。默认 `system` 会跟随操作系统外观，并在系统主题变化时即时更新。

## 损坏配置恢复

当 `app.settings` 不能反序列化时：

1. 在 `error.log` 记录解析错误，但不记录原始配置内容。
2. 尝试把原始内容保存为 `app.settings.corrupt.<unix-milliseconds>`。
3. 即使备份失败，也继续生成当前版本的默认配置。
4. 将默认配置重新写入 `app.settings`。
5. 只有默认配置无法持久化时，启动流程才返回错误。

备份记录保留在 SQLite 中，后续可用于诊断或人工恢复。

## 分层约束

- Command 层不直接访问 SQLite。
- Application 层定义 `ConfigRepository` 并通过 `ConfigService` 编排配置用例。
- Domain 层定义 `AppConfig` 和迁移规则。
- Infrastructure 层的 `DatabaseService` 实现 `ConfigRepository`，通过 SQLx 执行查询。
- ConfigService 单元测试使用内存仓储，不依赖 SQLite 或 Tauri State。
