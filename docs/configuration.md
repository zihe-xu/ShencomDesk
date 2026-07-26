# ShenDesk 配置系统

## 默认配置

```json
{
  "schemaVersion": 1,
  "theme": "dark",
  "language": "zh-CN",
  "autoStart": true
}
```

## 存储

配置以 JSON 形式存储在 SQLite 的 `app_config` 表中，键名为 `app.settings`。数据库文件位于 Tauri 应用数据目录的 `app.sqlite`。

## 生命周期

1. 应用启动时创建应用数据目录和 SQLite 数据库。
2. SQLx 自动执行 `src/infrastructure/database/migrations` 中的迁移。
3. `ConfigService::load` 读取 `app.settings`。
4. 配置缺失时写入默认值。
5. 旧配置会升级到当前 `schemaVersion`，升级结果会自动回写。
6. 不支持的主题和空语言值会被归一化。

## 分层约束

- Command 层不直接访问 SQLite。
- Application 层通过 `ConfigService` 编排配置用例。
- Domain 层定义 `AppConfig` 和迁移规则。
- Infrastructure 层通过 `DatabaseService` 执行 SQLx 查询。
