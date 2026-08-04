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

`build.rs` 使用 `AppManifest::commands` 注册所有前端可调用的应用命令。除健康、配置、任务、文件和图片命令外，插件能力只暴露以下 7 个入口：

- `install_plugin`
- `list_plugins`
- `get_plugin`
- `enable_plugin`
- `disable_plugin`
- `execute_plugin_command`
- `uninstall_plugin`

图片压缩只暴露一个本地入口：

- `compress_images`

自动更新只暴露 ShenDesk 自有的两个入口：

- `check_for_updates`
- `install_update`

Office 只暴露六个类型化入口：

- `get_office_engine_status`
- `create_office_document`
- `inspect_office_document`
- `apply_office_operations`
- `render_office_preview`
- `close_office_document`

主窗口没有 shell plugin 权限，也不存在接受 executable、环境变量、原始 argv、任意 batch verb/props 或 OfficeCLI path 表达式的 IPC。

WebView 不授予 `updater:*` 原生插件权限。Tauri 为这些自有命令生成对应的 allow/deny 权限。`capabilities/default.json` 只把 `allow-*` 权限授予标签为 `main` 的窗口；新增窗口默认不具备插件安装或执行权限。

新增 IPC 命令时，必须同时更新：

1. Rust `invoke_handler`
2. `build.rs` 的命令清单
3. 目标窗口 Capability 的权限清单
4. IPC 文档与自动化测试

## 认证边界

- WebView 只通过 `login` IPC 提交手机号和密码，不直接访问 Shencom 认证接口；通过 `get_auth_state` 恢复脱敏状态，通过 `logout` 清除本地会话。
- `SHENDESK_AUTH_ENVIRONMENT` 选择认证环境，支持 `test` 和 `production`，未设置时默认为 `test`；其他值会阻止认证服务初始化，避免静默连接错误环境。
- 测试环境使用 `https://tst-crm.shencom.cn` 与测试 `scid`，生产环境使用 `https://crm.shencom.cn` 与生产 `scid`。登录和刷新请求始终使用同一环境，请求整体超时为 15 秒。
- 服务端仅以 `errcode = "0000"` 表示成功；其他错误通过稳定的 `auth_failed` IPC 错误返回服务端用户提示。
- 网络失败、超时、非 200 状态、非 JSON 响应或缺少成功数据统一映射为脱敏的 `auth_unavailable`。
- 系统凭据库读取、写入或清理失败统一映射为脱敏的 `auth_storage_unavailable`，避免把本地 Keychain / Credential Manager 故障误报为认证服务不可用。
- Access Token、Refresh Token 和原始密码不写入日志，也不返回 WebView。IPC `AuthState` 只包含登录状态、用户资料和过期时间。
- WebView 仅在登录成功后把用户实际输入的手机号写入本地存储，用于退出登录后回填；不会保存密码，也不会使用 Token 或服务端用户资料中的手机号作为回填值。
- 完整 Token 会话以带版本号的 JSON 写入操作系统凭据库：macOS 使用 Keychain，Windows 使用 Credential Manager。服务名固定为 `com.shencom.shendesk.auth`；测试和生产环境分别使用 `test-session-v1` 与 `production-session-v1`，避免跨环境复用 Token。
- 应用启动时从系统凭据库恢复会话。Access Token 过期时，Rust Core 向当前环境的 `/service-uaa/auth/token-user/refresh` 提交 Refresh Token；成功后先安全持久化新 Token，再更新内存会话。服务端拒绝 Refresh Token 时清除本地会话；网络、HTTP 或响应解析失败时保留会话，允许下次恢复重试。
- Refresh Token 不进入 WebView、IPC 公共载荷或日志。损坏、版本不兼容或缺少可用 Refresh Token 的会话会被删除。凭据库不可用或恢复失败时应用仍以未登录状态启动。
- `logout` 仅执行本地会话清除，因为服务端撤销/登出接口尚未确认。
- `allow-login`、`allow-get-auth-state` 和 `allow-logout` 仅授予标签为 `main` 的窗口。

## Capability 启用策略

`tauri.conf.json` 显式启用 `default` Capability。这样未来即使 `capabilities/` 目录增加其他文件，也不会被构建自动全部启用。

## 本地文件命令

FileService 命令要求绝对路径，并限制读取大小、索引条目数和递归深度。文件内容不会写入日志或 EventBus；返回 WebView 的错误不会包含本地路径和原始 OS 错误。产品 UI 应通过可信系统选择流程取得路径。

## 本地图片压缩

- WebView 仅获得 `dialog:allow-open`，用于选择输入图片和输出目录；不授予通用文件系统插件权限。
- `compress_images` 只接受绝对路径、PNG/JPEG 输入和 1–100 的质量值，所有处理均在本地完成且不会发起网络请求。
- 输出使用排他创建。原文件、同名输出文件以及通过符号链接指向的已有文件都不会被覆盖；冲突项返回固定脱敏错误并继续批次。
- PNG 使用无损优化；JPEG 重编码可能移除 EXIF 等元数据，本期不承诺元数据保留。
- 逐项 Channel 错误和命令错误都不会包含绝对路径、编解码器内部文本或 OS 错误。

## OfficeCLI 进程边界

- OfficeCLI 仅由 Rust Infrastructure 从 ShenDesk 应用包内的固定位置解析；不读取 `PATH`，不回退到用户安装位置。
- 每个子进程固定设置 `OFFICECLI_SKIP_UPDATE=1`，调用方不能提供可执行文件路径、环境变量或原始 argv。
- stdout/stderr 均被捕获并限制大小；超时、取消、异常退出、超量输出和无效 JSON 只返回稳定脱敏错误。
- 日志不记录文档路径、Named Pipe、原始 stderr 或文档内容，EventBus 不发布 Office 文档生命周期细节。
- ShenDesk 只登记并清理自己打开的 session 和自己启动的临时子进程；退出时不扫描或终止用户的 OfficeCLI 进程。
- WebView 只接收八种稳定 Office 错误码和固定用户消息；绝对路径、临时目录、Named Pipe、原始 stderr 与文档内容不会进入 IPC 错误或进度 Channel。
- 创建和修改在 staging 文件上执行，只有 resident 成功 close 后才 no-clobber 提交；修改不会覆盖输入文件。PNG 预览限制为 16 MiB，临时文件读取后立即清理，只通过现有 `data:` 图片 CSP 展示，不加载 OfficeCLI HTML，也不启动 `watch`。
- OfficeCLI 是具有当前桌面用户原生进程权限的受信任供应链组件，不受 WebView CSP 或 WASM 沙箱限制。仓库以固定 tag、commit、源码 SHA-256、NuGet lock 和 .NET SDK 构建；最终 App/MSI 必须包含对应架构 sidecar 及 `LICENSE`、`NOTICE`、`THIRD-PARTY-NOTICES.txt`。
- 合并后与发布 smoke 都在 `OFFICECLI_SKIP_UPDATE=1` 下验证精确版本和 DOCX round trip。macOS 还要求 sidecar Developer ID 签名及 `allow-jit` entitlement；Windows 从 MSI 静默安装后的目录验收。检查发生在正常 Artifact 或 Release 资产上传之前。

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
## 自动更新供应链边界

- Tauri Updater 的签名验证不可关闭；ShenDesk 不提供未签名或 HTTP 降级。
- endpoint 固定为 GitHub Latest Release 的 HTTPS `latest.json`。
- 私钥只存在于 GitHub Actions Secret 和发布构建进程，不进入仓库、Artifact、日志或 WebView。
- 公钥只在标签发布编译时嵌入；普通构建缺少公钥时返回 `update_not_configured`。
- 下载 URL、签名和底层解析/安装错误不会通过 IPC 返回。
- `Signed desktop release` 不在 Pull Request 上运行，避免不受信任代码读取发布 Secret。
- release-only Tauri config 才启用 updater artifacts，普通 CI 不接触签名材料。
- Release 默认 Draft，核验多平台资产与 `latest.json` 后才发布。

更新签名只证明更新包由对应私钥签发，不代替操作系统平台信任。macOS 正式 Tag 发布另外强制 Developer ID Application 签名、Apple 公证与 Gatekeeper 验证；Windows Authenticode 仍需独立配置。密钥生成、发布步骤和轮换注意事项见 `docs/auto-update.md`。
