# Rust Core 分层架构

ShenDesk Rust Core 使用依赖倒置后的调用与实现关系：

```text
Tauri Command
      ↓
Application Service ──定义──> Repository / Runtime Port
      ↓                              ↑
Domain                       Infrastructure Adapter
                                      ↓
                 SQLx / notify / Wasmtime / OfficeCLI / Tauri Updater
```

## 目录职责

- `app`：启动、共享状态和生命周期。
- `commands`：接收 Tauri IPC 调用，只做输入输出适配。
- `application`：组织用例、服务流程，定义持久化/运行时端口并提供进程内 EventBus。
- `domain`：领域类型、规则与事件协议，不依赖 Tauri 或基础设施。
- `infrastructure`：数据库、缓存、文件、网络、系统、WASM Runtime、OfficeCLI Runtime 和 Tauri Updater 适配器。
- `utils`：错误类型等横切工具。

## 约束

1. Command 不直接执行 SQL、文件读写、网络请求或 WASM API。
2. Command 不实现业务规则。
3. Domain 不依赖 Tauri 和具体基础设施库。
4. Application 不导入 `infrastructure` 模块。
5. Infrastructure 实现 Application 定义的端口。
6. 应用启动资源由 `app::bootstrap` 统一注册。

## 配置存储示例

`application::config_repository::ConfigRepository` 定义配置键值的读取、写入与删除能力。`ConfigService` 只依赖该端口；`DatabaseService` 位于 Infrastructure 层，通过 SQLx/SQLite 实现。

```text
ConfigService
      ↓
ConfigRepository
      ↑
DatabaseService (SQLx / SQLite)
```

## 插件系统依赖倒置

`application::plugin_service` 定义两个端口：

- `PluginRepository`：读取来源包、安装、列举、状态持久化、模块读取和删除。
- `PluginRuntime`：校验模块、调用必需导出和调用可选生命周期导出。

`PluginService` 只依赖这些端口，负责 Manifest 规则、生命周期状态机、命令授权和 EventBus 事件。`LocalPluginRepository` 与 `WasmtimePluginRuntime` 位于 Infrastructure 层：

```text
Plugin Command
      ↓
PluginService ──────> PluginRepository / PluginRuntime
                           ↑                  ↑
                  LocalPluginRepository   WasmtimePluginRuntime
```

因此 Command 不接触 Wasmtime，Domain 不依赖 Wasmtime 类型，应用服务可使用内存仓储和记录型 Runtime 做单元测试。详见 `docs/plugin-system.md`。

## 自动更新依赖倒置

`application::update_service::UpdateBackend` 定义检查和安装端口。`UpdateService` 负责互斥操作、稳定错误和 `update_available` 领域事件；`TauriUpdateBackend` 位于 Infrastructure，持有不会跨 IPC 的 Tauri `Update`：

```text
Update Command
      ↓
UpdateService ──────> UpdateBackend
                          ↑
                  TauriUpdateBackend
                          ↓
                 tauri-plugin-updater
```

公钥、endpoint、签名验证、下载与安装细节都停留在 Infrastructure。Command 仅连接 Tauri Channel 和可选重启；Domain 只定义安全的版本元数据、进度事件和安装结果。详见 `docs/auto-update.md`。

## 模块事件通信

模块间通知通过 `application::event_bus::EventBus` 发送 Domain 层 `AppEvent`：

```text
Publisher → EventBus (tokio broadcast) → Subscriber(s)
```

EventBus 支持独立多订阅者、按 `EventKind` 过滤、单调 sequence 以及慢订阅者 lag 检测。它只用于进程内通知；需要跨重启恢复的状态由 SQLite 或模块自己的受管持久化实现保存。详见 `docs/event-bus.md`。

## 文件服务依赖倒置

`application::file_service::FileRepository` 定义读取、索引、监听、缓存失效和关闭能力。`LocalFileRepository` 位于 Infrastructure 层，使用 `std::fs`、`notify` 和 `moka` 实现；Tauri Command 只调用 `FileService`。详见 `docs/file-service.md`。

## 图片压缩依赖倒置

`application::image_service::ImageProcessor` 定义单文件处理端口。`ImageService` 负责请求校验、串行批处理、逐项进度和汇总；`LocalImageProcessor` 位于 Infrastructure 层，使用 `image` 和 `oxipng` 完成读取、编解码及排他写入：

```text
Image Command
      ↓
ImageService ──────> ImageProcessor
                          ↑
                 LocalImageProcessor
                 (image + oxipng)
```

Command 只负责 `spawn_blocking`、Channel 和稳定错误映射。单文件失败不会终止其余图片，已有输出文件不会被覆盖。

## Office Runtime 依赖倒置

`application::office_service::OfficeRuntime` 定义固定版本探测和文档
open/close 生命周期端口。`OfficeService` 负责格式与路径校验、同一标准化
文档的串行化、owned session 登记、取消和 best-effort close；测试可注入
recording runtime，不依赖真实 OfficeCLI。`OfficeCliRuntime` 位于
Infrastructure，只解析应用包内的固定 sidecar，并负责受限进程执行、输出
上限、超时、取消、退出状态与 JSON 校验。

`commands::office` 提供类型化的引擎状态、创建、结构化读取、白名单 batch、
PNG 预览和 owned document close，并通过阶段 Channel 报告长操作进度。创建
在同目录 staging 中完成并 close 后以 no-clobber 方式提交；修改先复制原文件，
只在 staging resident 成功 close 后提交到新的输出路径。预览写入单次调用的
受管临时目录，校验 PNG 和 16 MiB 上限后返回 data URL，目录随即清理。React 只
通过 `src/services/office.ts` 调用，请求无法传入二进制路径、环境变量、原始
argv、任意 batch verb 或 OfficeCLI path 表达式；不存在通用执行 IPC。
