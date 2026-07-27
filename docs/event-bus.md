# EventBus 事件系统

ShenDesk 使用进程内 EventBus 解耦 Rust Core 模块。事件模型位于 Domain 层，发布与订阅由 Application 层封装，底层使用 `tokio::sync::broadcast`。

## 结构

```text
Publisher module
      ↓ publish(AppEvent)
Application EventBus
      ↓ tokio broadcast
 ┌──────────────┬──────────────┬──────────────┐
Subscriber A    Subscriber B    Subscriber C
Task observer   File indexer    Diagnostics
```

相关代码：

- `domain/event.rs`：`AppEvent`、`EventKind`、`EventEnvelope`
- `application/event_bus.rs`：`EventBus`、`EventSubscriber`、接收错误
- `app/state.rs`：应用级共享 EventBus

Domain 事件不依赖 Tauri、Tokio、Wasmtime 或具体基础设施，因此发布方和订阅方只共享稳定的业务数据协议。

## 事件定义

当前事件类别：

- `application_ready`
- `application_exiting`
- `task_created`
- `task_started`
- `task_progressed`
- `task_finished`
- `file_changed`
- `plugin_installed`
- `plugin_enabled`
- `plugin_disabled`
- `plugin_executed`
- `plugin_removed`
- `user_logged_in`
- `update_available`

每次发布都会生成 envelope：

```json
{
  "sequence": 42,
  "publishedAtUnixMs": 1785110400000,
  "event": {
    "type": "plugin_removed",
    "payload": {
      "plugin_id": "com.shencom.hello"
    }
  }
}
```

`sequence` 在同一个应用进程内单调递增。EventBus 会串行化序号分配与 channel send，确保并发发布时所有订阅者观察到一致的事件顺序。

## 发布与订阅

```rust
state.event_bus().publish(AppEvent::PluginRemoved {
    plugin_id,
});

let mut subscriber = state.event_bus().subscribe_to([
    EventKind::TaskFinished,
    EventKind::FileChanged,
    EventKind::PluginExecuted,
]);
```

没有活跃订阅者是合法状态。每个 subscriber 拥有独立游标；一个订阅者处理较慢不会阻塞发布方或其他订阅者。

## 有界广播与 Lagged

默认容量为 256。EventBus 不保存无限历史，也不向新订阅者重放订阅前的事件。当未读事件被覆盖时，`recv` 或 `try_recv` 返回：

```rust
EventReceiveError::Lagged(skipped)
```

订阅模块必须根据业务语义处理：

- 可恢复状态：重新读取 SQLite 或当前服务快照。
- 仅诊断事件：记录 skipped 数量并继续。
- 必须逐条处理的工作：使用持久化队列或领域表，不能只依赖 EventBus。

## TaskManager 集成

```text
task_created
    ↓
task_started
    ↓
task_progressed (0..N)
    ↓
task_finished (success / failed / cancelled)
```

任务状态转换和事件发布在同一任务记录锁内完成，避免取消、进度和完成并发时产生倒序事件。

## PluginService 集成

```text
plugin_installed (disabled)
    ↓
plugin_enabled
    ↓
plugin_executed (0..N)
    ↓
plugin_disabled
    ↓
plugin_removed
```

事件只包含 Manifest、持久化状态、命令名、返回码与 fuel 消耗等领域数据，不包含来源路径、WASM 字节、编译诊断或 trap 详情。启动恢复失败的插件被持久化为 disabled，并发布 `plugin_disabled`；应用退出时的临时 runtime stop 不改变持久化 enabled 偏好，因此不伪造状态变更事件。

## UpdateService 集成

检查到高于当前版本的签名发布时，`UpdateService` 发布：

```text
update_available { version }
```

事件只包含目标 SemVer，不包含下载 URL、签名、清单原文或私钥。没有更新时不发布；安装进度是面向发起 WebView 的 IPC Channel，不进入全局 EventBus，避免高频下载块占用广播容量。

## 应用生命周期

- 完成共享状态注册后发布 `application_ready`。
- 开始退出清理时发布 `application_exiting`。
- 随后 PluginService 停止运行态，FileService 停止 watcher，TaskManager 取消非终态任务。

EventBus 是进程内通信设施；应用退出后事件不会保留。插件启用状态由受管插件状态文件保存，其他需要跨重启恢复的数据仍应持久化到 SQLite 或专用领域存储。

## 约束

1. 模块通过 `AppEvent` 通信，不把闭包、Tauri handle 或基础设施对象放入事件。
2. 事件 payload 不得包含 token、密钥、本地来源路径或不必要的敏感信息。
3. Subscriber 不应在接收循环中执行长时间阻塞工作；重任务应提交给 TaskManager。
4. Subscriber 必须显式处理 `Lagged` 和 `Closed`。
5. EventBus 不代替持久化任务队列、数据库事务或跨进程消息系统。

## 测试

测试覆盖多 subscriber fan-out、类别过滤、lag 检测、有序 sequence、零 subscriber 发布、稳定 wire format、TaskManager 生命周期、插件生命周期事件顺序、更新可用事件，以及 AppState 内核心服务共享同一 EventBus。
