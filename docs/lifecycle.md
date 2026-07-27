# 应用生命周期

ShenDesk 使用 Tauri `App::run` 的事件回调统一处理运行时生命周期。

## 启动顺序

1. 创建应用数据目录。
2. 初始化 tracing 日志并托管 `LoggingGuards`。
3. 创建 SQLite 连接池并执行 SQLx Migration。
4. 加载、迁移或恢复应用配置。
5. 注册包含 EventBus 和 TaskManager 的 Tauri 管理状态。
6. 发布 `application_ready`。
7. 记录 `application.ready` 操作日志。

## 退出顺序

资源清理只在 `RunEvent::Exit` 执行：

1. 记录 `application.exit=requested`。
2. 发布 `application_exiting`。
3. TaskManager 停止接收任务，取消非终态任务并发布 `task_finished(cancelled)`。
4. 执行 `PRAGMA wal_checkpoint(TRUNCATE)`。
5. 关闭 SQLx 连接池，阻止新的数据库操作。
6. 记录数据库和应用退出结果。
7. 丢弃 tracing `WorkerGuard`，刷新三个非阻塞日志 Writer。

`RunEvent::ExitRequested` 表示应用即将退出，但未来可能被确认对话框、后台任务或未保存数据阻止。因此该事件暂不执行不可逆的资源释放。

## 扩展约束

后续 TaskManager、文件监听器和插件运行时必须把停止逻辑接入 `app::lifecycle::on_exit`，并遵循：

```text
停止接受新任务
    ↓
取消或完成运行中任务
    ↓
刷新业务状态
    ↓
Checkpoint / 关闭数据库
    ↓
刷新日志
```

日志 Worker 必须最后关闭，以便前面的每个清理阶段都能记录成功或失败信息。
