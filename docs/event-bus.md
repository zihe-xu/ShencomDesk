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

Domain 事件不依赖 Tauri、Tokio 或具体基础设施，因此发布方和订阅方只共享稳定的业务数据协议。

## 事件定义

当前事件类别：

- `application_ready`
- `application_exiting`
- `task_created`
- `task_started`
- `task_progressed`
- `task_finished`
- `file_changed`
- `user_logged_in`
- `update_available`

每次发布都会生成一个 envelope：

```json
{
  "sequence": 42,
  "publishedAtUnixMs": 1785110400000,
  "event": {
    "type": "update_available",
    "payload": {
      "version": "2.0.0"
    }
  }
}
```

`sequence` 在同一个应用进程内单调递增。EventBus 会串行化序号分配与 channel send，确保并发发布时所有订阅者观察到一致的事件顺序。

## 发布

```rust
use crate::domain::event::AppEvent;

state.event_bus().publish(AppEvent::FileChanged {
    path: path.to_string_lossy().into_owned(),
});
```

没有活跃订阅者是合法状态。发布不会因为接收者数量为零而失败，也不会等待订阅者完成处理。

## 订阅

订阅全部事件：

```rust
let mut subscriber = state.event_bus().subscribe();

while let Ok(envelope) = subscriber.recv().await {
    handle(envelope.event);
}
```

按类别订阅：

```rust
use crate::domain::event::EventKind;

let mut subscriber = state.event_bus().subscribe_to([
    EventKind::TaskFinished,
    EventKind::FileChanged,
]);
```

每个 subscriber 拥有独立游标。一个订阅者处理较慢不会阻塞发布方，也不会阻塞其他订阅者。

## 有界广播与 Lagged

默认容量为 256。EventBus 不保存无限历史，也不向新订阅者重放订阅前的事件。

当订阅者落后并且其未读事件已被覆盖时，`recv` 或 `try_recv` 返回：

```rust
EventReceiveError::Lagged(skipped)
```

订阅模块必须根据业务语义处理：

- 可恢复状态：重新读取 SQLite 或当前服务快照
- 仅诊断事件：记录 skipped 数量并继续
- 必须逐条处理的工作：不要只依赖 EventBus，应使用持久化队列或领域表

当所有 publisher 被释放时返回 `EventReceiveError::Closed`。

## TaskManager 集成

TaskManager 使用应用级 EventBus 发布完整生命周期：

```text
task_created
    ↓
task_started
    ↓
task_progressed (0..N)
    ↓
task_finished (success / failed / cancelled)
```

任务状态转换和事件发布在同一任务记录锁内完成，从而避免取消、进度和完成并发时产生倒序事件。事件 payload 使用 `TaskSnapshot`，订阅者无需再直接访问 TaskManager 内部状态。

## 应用生命周期

- 完成共享状态注册后发布 `application_ready`
- 开始退出清理时发布 `application_exiting`
- 随后 TaskManager 取消非终态任务并发布对应 `task_finished`

EventBus 是进程内通信设施；应用退出后事件不会保留。需要跨重启恢复的数据必须写入 SQLite。

## 约束

1. 模块通过 `AppEvent` 通信，不把闭包、Tauri handle 或基础设施对象放入事件。
2. 事件 payload 不得包含 token、密钥或不必要的本地敏感信息。
3. Subscriber 不应在接收循环中执行长时间阻塞工作；重任务应提交给 TaskManager。
4. Subscriber 必须显式处理 `Lagged` 和 `Closed`。
5. EventBus 不代替持久化任务队列、数据库事务或跨进程消息系统。

## 测试

测试覆盖：

- 多 subscriber fan-out
- 按 `EventKind` 过滤
- 慢 subscriber 的 lag 检测
- clone publisher 共享有序 sequence
- 零 subscriber 发布
- Event wire format
- TaskManager 生命周期事件顺序
- AppState 中 TaskManager 与其他模块共享同一 EventBus
