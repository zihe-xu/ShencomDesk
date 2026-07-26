# SQLite 运行策略

ShenDesk 使用 SQLx 管理本地 SQLite 数据库。桌面应用的文件型数据库采用以下固定运行参数：

| 参数 | 值 | 目的 |
|---|---:|---|
| `journal_mode` | `WAL` | 允许读操作与写操作更好地并行 |
| `synchronous` | `NORMAL` | 在 WAL 模式下平衡持久性与性能 |
| `busy_timeout` | 5000 ms | 锁竞争时等待，而不是立即返回 `SQLITE_BUSY` |
| `wal_autocheckpoint` | 1000 页 | 控制 WAL 文件自动 checkpoint |
| 连接池上限 | 5 | 为 UI、配置和后台任务保留有限并发 |
| `foreign_keys` | ON | 强制执行外键约束 |

## 写入模型

SQLite 同一时刻仍然只有一个写者。WAL 和 busy timeout 用于降低短时锁竞争，但不能把 SQLite 变成多写者数据库。

ShenDesk 的约束：

1. 写事务保持短小，不在事务中执行网络、文件扫描或 AI 计算。
2. Command 层不直接写数据库；写入由 Application Service 组织。
3. TaskManager 引入后，批量写入应通过受控 Worker 或单写者队列串行提交。
4. 长时间读取应避免持有事务，以免阻塞 checkpoint。
5. 应用退出时执行数据库关闭和必要的 WAL checkpoint，由生命周期模块统一协调。

## 测试

数据库测试使用临时文件型 SQLite，验证：

- WAL 实际启用
- `synchronous=NORMAL`
- 5 秒锁等待
- 外键约束启用
- 1000 页自动 checkpoint
- 多个短写任务不会产生 `SQLITE_BUSY`

内存 SQLite 仅用于不依赖 WAL 的服务单元测试，并固定为单连接，确保所有操作访问同一个内存数据库。
