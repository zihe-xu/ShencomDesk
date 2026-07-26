# TaskManager 任务系统

ShenDesk 的 TaskManager 为本地后台工作提供有界队列、worker、进度快照、协作式取消和生命周期关闭能力。

## 结构

```text
React service
  → Tauri task command
  → TaskService
  → TaskManager
      ├── bounded FIFO queue
      ├── worker pool
      ├── task snapshots
      ├── progress context
      └── cancellation flag
```

任务域类型位于 `domain/task.rs`，队列与执行编排位于 `application/task_service.rs`。Tauri 运行时状态通过 `AppState` 持有一个 TaskManager 实例。

TaskManager 复用 `tauri::async_runtime` 提供的 Tokio channel、worker spawn 和 blocking worker pool，不引入第二套运行时。

## 状态机

```text
pending → running → success
                  ↘ failed
pending/running → cancelled
```

终态为：

- `success`
- `failed`
- `cancelled`

终态不会被后续进度上报覆盖。取消与完成发生竞争时，先获得任务记录写锁的终态转换生效；已取消任务不会再变为成功或失败。

## 队列与 Worker

默认配置：

- 队列容量：128
- worker 数量：2

队列使用 FIFO 顺序。单 worker 时严格按入队顺序开始执行；多 worker 时按入队顺序取出，但任务完成顺序由实际执行时间决定。

当队列已满、已关闭或应用正在退出时，提交返回 `TaskManagerError::QueueUnavailable`，不会留下不可执行的任务记录。

## 进度

任务通过 `TaskContext::report_progress(completed)` 上报绝对完成量。TaskManager 会：

- 把完成量限制在声明的 `total` 内
- 使用整数运算计算 `percentage`
- 忽略已取消或已进入终态的进度

快照示例：

```json
{
  "id": "task-0000000000000001",
  "name": "index files",
  "state": "running",
  "progress": {
    "completed": 42,
    "total": 100,
    "percentage": 42
  },
  "error": null
}
```

## 取消语义

取消是协作式的：

1. `cancel_task` 立即把对外快照设置为 `cancelled`
2. cancellation flag 通知正在执行的任务停止
3. 任务实现必须定期调用 `TaskContext::is_cancelled()` 或检查 `report_progress` 的返回值

Rust 无法安全地强制终止任意正在运行的阻塞代码，因此新增任务处理器不得长时间忽略取消标记。

## IPC 命令

| Command | 输入 | 输出 |
|---|---|---|
| `create_task` | `{ request: { name, totalSteps, stepDelayMs? } }` | `TaskSnapshot` |
| `get_task_status` | `{ taskId }` | `TaskSnapshot` |
| `list_tasks` | 无 | `TaskSnapshot[]` |
| `cancel_task` | `{ taskId }` | `TaskSnapshot` |

`create_task` 当前提供可观察进度与取消行为的通用后台任务入口。内部文件扫描、同步、下载和 AI 处理模块可直接通过 `TaskManager::submit` 注册具体工作。

为防止 IPC 滥用，通用任务入口限制：

- 名称最多 128 个字符
- 总步骤为 1 到 10,000
- 单步延迟最多 1 秒
- 声明总时长最多 10 分钟

## 前端使用

```ts
import {
  cancelTask,
  createTask,
  getTaskStatus,
  listTasks,
} from "@/services/tasks";

const task = await createTask({
  name: "index files",
  totalSteps: 100,
  stepDelayMs: 25,
});

const current = await getTaskStatus(task.id);
const all = await listTasks();
await cancelTask(current.id);
```

## 生命周期与持久化

应用退出时会先停止接收新任务并取消所有非终态任务，再关闭数据库和刷新日志。

当前任务快照仅保存在内存中，应用重启后不会恢复。需要断点续传的下载、同步等业务应在各自领域表中持久化业务状态，并在启动阶段重新提交任务。

## 测试

Rust 测试覆盖：

- FIFO 队列顺序
- worker 成功执行
- 进度与百分比
- 失败消息
- 运行中取消
- shutdown 后拒绝新任务
- IPC 参数边界
- 稳定状态序列化
