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
7. 配置 JSON 损坏时执行恢复流程，应用不会因为解析错误而停止启动。

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
- Application 层通过 `ConfigService` 编排配置用例。
- Domain 层定义 `AppConfig` 和迁移规则。
- Infrastructure 层通过 `DatabaseService` 执行 SQLx 查询。
