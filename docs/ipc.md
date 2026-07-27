# Tauri IPC Command Layer

ShenDesk 使用 Tauri Command 作为 React 与 Rust Core 之间的薄传输边界。

## 已注册命令

| Command | 输入 | 输出 |
|---|---|---|
| `health_check` | 无 | `HealthStatus` |
| `get_config` | 无 | `AppConfig` |
| `save_config` | `{ config: AppConfig }` | 归一化后的 `AppConfig` |
| `reset_config` | 无 | 默认 `AppConfig` |
| `create_task` | `{ request: CreateTaskRequest }` | `TaskSnapshot` |
| `get_task_status` | `{ taskId: string }` | `TaskSnapshot` |
| `list_tasks` | 无 | `TaskSnapshot[]` |
| `cancel_task` | `{ taskId: string }` | `TaskSnapshot` |
| `read_text_file` | `{ request: { path, maxBytes? } }` | `FileReadResult` |
| `index_files` | `{ request: { root, maxEntries?, maxDepth? } }` | `FileIndex` |
| `start_file_watch` | `{ request: { path, recursive? } }` | `FileWatch` |
| `stop_file_watch` | `{ watchId: string }` | watch ID |
| `clear_file_cache` | 无 | 无 |

## 分层规则

```text
React Service
  → Tauri Command
  → Application Service
  → Domain / Repository Port
  → Infrastructure Adapter
```

Command 可以反序列化传输参数、读取 Tauri 托管状态、验证传输边界和转换错误，但不得直接执行 SQL。Task Command 把经过验证的请求委托给 `TaskService` 和 `TaskManager`。

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
| `task_not_found` | 指定任务不存在 |
| `task_queue_unavailable` | 任务队列已满、已关闭或正在退出 |
| `file_not_found` | 指定文件或目录不存在 |
| `file_access_denied` | OS 拒绝文件访问 |
| `file_too_large` | 文本文件超过读取上限 |
| `file_not_text` | 文件不是有效 UTF-8 文本 |
| `file_watch_unavailable` | 平台文件监听无法启动 |
| `file_watch_not_found` | 指定 watch ID 不存在 |
| `file_operation_failed` | 其他文件操作失败 |
| `validation_failed` | 输入未通过验证 |
| `unknown_error` | 前端收到未知或不可信错误载荷 |

## 信息脱敏

Rust 端把原始 `AppError` 和 TaskManager 诊断写入日志，其中可以包含 SQLx 错误、数据库路径和内部执行上下文。

发送给 WebView 的 `IpcError` 只包含稳定错误码与面向用户的固定消息，不包含：

- 本地文件路径
- SQL 或数据库内部文本
- Rust 类型和堆栈细节
- 配置原始内容
- worker panic 或任务内部错误细节

前端只接受白名单中的错误码。未知对象、原生 `Error` 或空消息都会被归一化为：

```json
{
  "code": "unknown_error",
  "message": "操作失败，请重试。"
}
```

## 前端使用

配置：

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

任务：

```ts
const task = await createTask({
  name: "index files",
  totalSteps: 100,
  stepDelayMs: 25,
});

const current = await getTaskStatus(task.id);
await cancelTask(current.id);
```

所有原始 `invoke` 调用集中在 `src/services/tauri.ts`。配置、任务和文件的类型安全封装分别位于 `src/services/config.ts`、`src/services/tasks.ts` 和 `src/services/files.ts`。纯错误映射位于 `src/services/tauri-errors.ts`，可脱离 Tauri Runtime 使用 Node 内置测试执行。

## 测试

CI 同时执行：

- Rust：验证数据库错误脱敏、稳定配置/任务错误码、任务参数边界，以及 TaskManager 的队列、进度、失败、取消和关闭行为。
- TypeScript：验证已知配置/任务/文件错误保留、未知错误脱敏、空消息拒绝，并执行完整前端构建。
