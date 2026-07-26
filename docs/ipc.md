# Tauri IPC Command Layer

ShenDesk 使用 Tauri Command 作为 React 与 Rust Core 之间的薄传输边界。

## 已注册命令

| Command | 输入 | 输出 |
|---|---|---|
| `health_check` | 无 | `HealthStatus` |
| `get_config` | 无 | `AppConfig` |
| `save_config` | `{ config: AppConfig }` | 归一化后的 `AppConfig` |
| `reset_config` | 无 | 默认 `AppConfig` |

## 分层规则

```text
React Service
  → Tauri Command
  → Application Service
  → Domain / Repository Port
  → Infrastructure Adapter
```

Command 可以反序列化传输参数、读取 Tauri 托管状态和转换错误，但不得包含业务规则或直接执行 SQL。

## 稳定错误协议

可失败命令返回经过脱敏的结构：

```json
{
  "code": "database_unavailable",
  "message": "本地数据服务暂时不可用，请重试。"
}
```

当前稳定错误码：

| Code | 场景 |
|---|---|
| `database_unavailable` | SQLite 或本地数据服务不可用 |
| `config_load_failed` | 配置读取或恢复失败 |
| `config_save_failed` | 配置保存失败 |
| `config_reset_failed` | 默认配置恢复失败 |
| `validation_failed` | 输入未通过验证 |
| `unknown_error` | 前端收到未知或不可信错误载荷 |

## 信息脱敏

Rust 端把原始 `AppError` 写入 `error.log.*`，其中可以包含 SQLx 错误、数据库路径和诊断上下文。

发送给 WebView 的 `IpcError` 只包含稳定错误码与面向用户的固定消息，不包含：

- 本地文件路径
- SQL 或数据库内部文本
- Rust 类型和堆栈细节
- 配置原始内容

前端只接受白名单中的错误码。未知对象、原生 `Error` 或空消息都会被归一化为：

```json
{
  "code": "unknown_error",
  "message": "操作失败，请重试。"
}
```

## 前端使用

```ts
try {
  const config = await getConfig();
  await saveConfig({ ...config, theme: "light" });
} catch (error) {
  if (error instanceof ShenDeskIpcError) {
    console.log(error.code);
  }
}
```

所有原始 `invoke` 调用集中在 `src/services/tauri.ts`。纯错误映射位于 `src/services/tauri-errors.ts`，可脱离 Tauri Runtime 使用 Node 内置测试执行。

## 测试

CI 同时执行：

- Rust：验证数据库错误不会泄露路径或 SQLite 文本，并检查配置场景错误码。
- TypeScript：验证已知错误保留、未知错误脱敏、空消息拒绝。
