# 应用生命周期

ShenDesk 使用 Tauri `App::run` 的事件回调统一处理运行时生命周期。

## 启动顺序

1. 创建应用数据目录。
2. 初始化 tracing 日志并托管 `LoggingGuards`。
3. 创建 SQLite 连接池并执行 SQLx Migration。
4. 加载、迁移或恢复应用配置。
5. 从应用可执行文件所在目录解析固定 OfficeCLI sidecar，并在最多 2 秒内精确探测其版本；缺失、超时或版本不匹配时记录脱敏的 unavailable 状态。
6. 初始化受管插件目录和 Wasmtime Runtime。
7. 恢复上次持久化为 enabled 的插件；校验或 enable hook 失败的插件自动隔离为 disabled。
8. 初始化 UpdateService；发布构建读取编译时公钥，普通构建保持安全未配置状态。
9. 注册 EventBus、TaskManager、FileService、OfficeService、PluginService 和 UpdateService 的 Tauri 管理状态。
10. 发布 `application_ready`。
11. 记录 `application.ready` 操作日志。

## 退出顺序

资源清理只在 `RunEvent::Exit` 执行：

1. 记录 `application.exit=requested`。
2. 发布 `application_exiting`。
3. OfficeService 停止接受新操作，取消自己登记的临时子进程，并 best-effort close 自己打开的文档 session。
4. PluginService 对 enabled 插件执行 best-effort disable hook，但保留其跨重启启用偏好。
5. FileService 停止所有文件 watcher 并清空内存缓存。
6. TaskManager 停止接收任务，取消非终态任务并发布 `task_finished(cancelled)`。
7. 执行 `PRAGMA wal_checkpoint(TRUNCATE)`。
8. 关闭 SQLx 连接池，阻止新的数据库操作。
9. 记录数据库和应用退出结果。
10. 丢弃 tracing `WorkerGuard`，刷新三个非阻塞日志 Writer。

`RunEvent::ExitRequested` 表示应用即将退出，但未来可能被确认对话框、后台任务或未保存数据阻止。因此该事件暂不执行不可逆的资源释放。

更新安装在 macOS 请求重启时调用 `AppHandle::request_restart`，仍会进入同一 `RunEvent::Exit` 清理顺序，而不是绕过数据库 checkpoint、插件 stop 和日志刷新。Windows 安装器可能由系统安装流程直接终止应用，因此更新 Command 在下载完成时先发送 `finished` Channel 事件，并将平台行为记录在更新文档。

插件 hook 先于共享文件和任务服务停止执行，为未来显式授权的宿主能力保留正确顺序；ABI v1 当前不开放任何宿主 import。单个插件关闭失败只记录诊断，不阻止其他插件和核心资源释放。

## 扩展约束

后续后台服务和插件运行时必须把停止逻辑接入 `app::lifecycle::on_exit`，并遵循：

```text
停止接受新任务
    ↓
取消 owned OfficeCLI 子进程并关闭 owned session
    ↓
停止插件运行态
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
