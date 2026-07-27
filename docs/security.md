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

`build.rs` 使用 `AppManifest::commands` 注册所有前端可调用的应用命令。除健康、配置、任务和文件命令外，插件能力只暴露以下 7 个入口：

- `install_plugin`
- `list_plugins`
- `get_plugin`
- `enable_plugin`
- `disable_plugin`
- `execute_plugin_command`
- `uninstall_plugin`

Tauri 为这些命令生成对应的 allow/deny 权限。`capabilities/default.json` 只把 `allow-*` 权限授予标签为 `main` 的窗口；新增窗口默认不具备插件安装或执行权限。

新增 IPC 命令时，必须同时更新：

1. Rust `invoke_handler`
2. `build.rs` 的命令清单
3. 目标窗口 Capability 的权限清单
4. IPC 文档与自动化测试

## Capability 启用策略

`tauri.conf.json` 显式启用 `default` Capability。这样未来即使 `capabilities/` 目录增加其他文件，也不会被构建自动全部启用。

## 本地文件命令

FileService 命令要求绝对路径，并限制读取大小、索引条目数和递归深度。文件内容不会写入日志或 EventBus；返回 WebView 的错误不会包含本地路径和原始 OS 错误。产品 UI 应通过可信系统选择流程取得路径。

## WASM 插件沙箱

插件 ABI v1 使用默认拒绝策略：

- 不启用 WASI，不提供文件、网络、环境变量、进程或密钥能力。
- 模块声明任何 host import 都会在安装校验阶段被拒绝。
- Manifest 最大 64 KiB，模块最大 16 MiB，命令最多 64 个。
- 每次验证和调用创建独立 Wasmtime Store 与实例。
- 每次调用最多 10,000,000 fuel；无限执行会 trap。
- 每个 Store 的线性内存上限 64 MiB，表元素上限 10,000，实例/内存/表数量各 1。
- WASM 栈上限 512 KiB，资源增长失败直接 trap。
- 受管插件目录拒绝符号链接文件，并要求目录名与 Manifest ID 一致。
- WebView 只接收固定插件错误码，Manifest 路径、编译细节和 trap 文本仅写入本地日志。

沙箱不替代供应链信任。未来插件市场还必须增加发布者签名、内容哈希、权限声明、撤销与企业策略；详见 `docs/plugin-system.md`。
