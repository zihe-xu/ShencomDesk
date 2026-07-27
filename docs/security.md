# Tauri 安全基线

ShenDesk 使用 Tauri 2 的 CSP、Permissions 与 Capabilities 建立 WebView 到 Rust Core 的最小权限边界。

## Content Security Policy

生产构建只允许加载应用自身、本地 asset 协议、Tauri IPC，以及图片和字体所需的本地/内嵌资源。

开发环境通过 `devCsp` 额外允许：

- Vite 开发服务器 `http://localhost:1420`
- HMR WebSocket `ws://localhost:1421`
- 开发工具链所需的 `unsafe-eval`

生产 CSP 不允许远程脚本、不允许对象嵌入，也不允许页面被其他页面以 frame 方式嵌入。

## 自定义命令权限

`build.rs` 使用 `AppManifest::commands` 注册所有前端可调用的应用命令：

- `health_check`
- `get_config`
- `save_config`
- `reset_config`
- `create_task`
- `get_task_status`
- `list_tasks`
- `cancel_task`
- `read_text_file`
- `index_files`
- `start_file_watch`
- `stop_file_watch`
- `clear_file_cache`

Tauri 为这些命令生成对应的 allow/deny 权限。`capabilities/default.json` 仅把以下 allow 权限授予标签为 `main` 的窗口：

- `allow-health-check`
- `allow-get-config`
- `allow-save-config`
- `allow-reset-config`
- `allow-create-task`
- `allow-get-task-status`
- `allow-list-tasks`
- `allow-cancel-task`
- `allow-read-text-file`
- `allow-index-files`
- `allow-start-file-watch`
- `allow-stop-file-watch`
- `allow-clear-file-cache`

新增窗口默认不会获得这些权限。新增 IPC 命令时，必须同时更新：

1. Rust `invoke_handler`
2. `build.rs` 的命令清单
3. 目标窗口 Capability 的权限清单
4. IPC 文档与自动化测试

## Capability 启用策略

`tauri.conf.json` 显式启用 `default` Capability。这样未来即使 `capabilities/` 目录增加其他文件，也不会被构建自动全部启用。

## 本地文件命令

FileService 命令要求绝对路径，并限制读取大小、索引条目数和递归深度。文件内容不会写入日志或 EventBus；返回 WebView 的错误不会包含本地路径和原始 OS 错误。产品 UI 应通过可信系统选择流程取得路径。
