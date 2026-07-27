# FileService 文件服务

ShenDesk 的 FileService 为本地优先桌面能力提供受限文本读取、目录索引、文件监听和内存缓存。Application 层定义端口与用例，Infrastructure 层使用标准文件 API、`notify` 和 `moka` 实现。

## 结构

```text
React service
  → Tauri file command
  → FileService
  → FileRepository port
  → LocalFileRepository
      ├── std::fs read/index
      ├── moka text cache
      ├── notify watcher
      └── EventBus file_changed publisher
```

相关代码：

- `domain/file.rs`：文件条目、读取结果、索引、监听和变更类型
- `application/file_service.rs`：FileService、输入边界和 `FileRepository` 端口
- `infrastructure/filesystem/mod.rs`：本地文件系统适配器
- `commands/file.rs`：Tauri IPC 适配
- `src/services/files.ts`：前端类型安全调用

## 文件读取

`read_text_file` 只读取 UTF-8 文本，并执行双重大小检查：

1. 读取前检查 metadata 大小
2. 通过 `Read::take(max + 1)` 防止读取期间文件增长绕过限制

默认上限为 4 MiB，调用方可降低或提高，但硬上限为 16 MiB。目录、无效 UTF-8、权限不足、文件不存在和超限分别映射为稳定且脱敏的 IPC 错误。

```ts
const result = await readTextFile({
  path: "/absolute/path/to/notes.md",
  maxBytes: 2 * 1024 * 1024,
});

console.log(result.content, result.fromCache);
```

## 文件缓存

文本缓存使用 `moka::sync::Cache`：

- 最大 256 个文件
- TTL 5 分钟
- key 为规范化绝对路径
- value 同时保存文件长度和纳秒级修改时间签名

只有签名与当前 metadata 一致时才返回缓存值。文件监听收到任意变更后会清空该小型有界缓存，保证重命名、目录删除和旧路径无法 canonicalize 时也不会返回陈旧内容。

`clear_file_cache` 可由前端或诊断流程主动失效全部缓存。

## 文件索引

`index_files` 构建确定性的内存快照：

- 每层目录按路径排序
- 默认最多 5,000 条，硬上限 20,000 条
- 默认最大深度 16，硬上限 64
- 不跟随符号链接，避免目录环
- 无法读取的子条目会记录诊断并跳过
- 达到条目上限时返回 `truncated: true`

索引不是数据库。需要全文搜索、跨重启恢复或增量查询时，应把 FileService 结果写入 SQLite/FTS5 的领域索引表。

## 文件监听

`start_file_watch` 使用平台推荐的 `notify::RecommendedWatcher`，支持：

- 单文件非递归监听
- 目录非递归监听
- 目录递归监听
- 多个独立 watch registration
- 通过 watch ID 显式停止

每个变更会发布 Domain 事件：

```json
{
  "type": "file_changed",
  "payload": {
    "change": {
      "watchId": "watch-0000000000000001",
      "path": "/absolute/path/to/notes.md",
      "kind": "modified"
    }
  }
}
```

变更类别为 `created`、`modified`、`removed` 或 `other`。底层平台可能合并或重复事件，因此 Subscriber 必须把通知视为“重新读取当前状态”的触发器，而不是持久化审计日志。

## IPC 命令

| Command | 输入 | 输出 |
|---|---|---|
| `read_text_file` | `{ request: { path, maxBytes? } }` | `FileReadResult` |
| `index_files` | `{ request: { root, maxEntries?, maxDepth? } }` | `FileIndex` |
| `start_file_watch` | `{ request: { path, recursive? } }` | `FileWatch` |
| `stop_file_watch` | `{ watchId }` | 停止的 watch ID |
| `clear_file_cache` | 无 | 无 |

所有路径必须为绝对路径，最大 4,096 个字符。Command 不直接操作文件系统，只调用 `FileService`。

## 生命周期

应用退出顺序：

```text
application_exiting
  → 停止所有文件 watcher
  → 清空文件缓存
  → 停止 TaskManager
  → 关闭 SQLite
  → 刷新日志
```

watch registration 和缓存都只存在于当前进程。应用重启后，业务模块必须根据持久化配置重新注册监听。

## 安全边界

- 文件命令只授权给 `main` 窗口
- CSP 不允许远程脚本
- IPC 错误不会返回本地路径、OS 错误或内部实现细节
- 文本读取和索引均有硬限制
- 不跟随索引中的符号链接
- 文件内容不会写入日志或 EventBus

当前 API 接受调用方提供的绝对路径。产品界面应通过可信的系统文件选择流程获取路径，不应让不可信网页内容直接构造任意路径。

## 测试

Rust 测试覆盖：

- UTF-8 读取和缓存命中
- metadata 变化后的缓存失效
- 超限和非 UTF-8 拒绝
- 递归索引与截断
- watcher 事件和停止语义
- FileService 输入上限
- EventBus `file_changed` 发布
- IPC 请求字段和错误脱敏
