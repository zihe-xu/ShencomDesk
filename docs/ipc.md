# Tauri IPC Command Layer

ShenDesk 使用 Tauri Command 作为 React 与 Rust Core 之间的薄传输边界。

## 已注册命令

| Command | 输入 | 输出 |
|---|---|---|
| `login` | `{ request: { username, password } }` | `AuthState` |
| `get_auth_state` | 无 | `AuthState` |
| `logout` | 无 | `AuthState` |
| `health_check` | 无 | `HealthStatus` |
| `get_config` | 无 | `AppConfig` |
| `save_config` | `{ config: AppConfig }` | 归一化后的 `AppConfig` |
| `reset_config` | 无 | 默认 `AppConfig` |
| `create_task` | `{ request: CreateTaskRequest }` | `TaskSnapshot` |
| `get_task_status` | `{ taskId: string }` | `TaskSnapshot` |
| `list_tasks` | 无 | `TaskSnapshot[]` |
| `cancel_task` | `{ taskId: string }` | `TaskSnapshot` |
| `read_text_file` | `{ request: { path, maxBytes? } }` | `FileReadResult` |
| `index_files` | `{ request: { root, maxEntries?, maxDepth? } }` | `FileIndex` |
| `start_file_watch` | `{ request: { path, recursive? } }` | `FileWatch` |
| `stop_file_watch` | `{ watchId: string }` | watch ID |
| `clear_file_cache` | 无 | 无 |
| `compress_images` | `{ request: { items, outputDir, quality }, onProgress: Channel }` | `CompressImagesResult` |
| `install_plugin` | `{ request: { manifestPath } }` | `PluginSnapshot` |
| `list_plugins` | 无 | `PluginSnapshot[]` |
| `get_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `enable_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `disable_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `execute_plugin_command` | `{ request: { pluginId, command } }` | `PluginExecution` |
| `uninstall_plugin` | `{ pluginId }` | plugin ID |
| `check_for_updates` | 无 | `UpdateInfo | null` |
| `install_update` | `{ request: { restart? }, onProgress: Channel }` | `UpdateInstallResult` |

## 分层规则

```text
React Service
  → Tauri Command
  → Application Service
  → Domain / Repository Port
  → Infrastructure Adapter
```

Command 可以反序列化传输参数、读取 Tauri 托管状态、验证传输边界和转换错误，但不得直接执行 SQL、文件 I/O、WASM 运行时逻辑或更新网络请求。插件 Command 只把请求委托给 `PluginService`；更新 Command 只把检查与安装委托给 `UpdateService`，不直接构建 updater client、读取更新地址或处理签名。

认证 Command 只把请求委托给 `AuthService`。`AuthService` 负责输入归一化、成功码、HTTP 状态语义、会话状态和登录/登出事件；Shencom 请求地址、请求头、超时与 JSON 解码由 `infrastructure/auth` 网络适配器负责，系统凭据库存取由同模块的 `KeyringAuthSessionStore` 负责。

`AuthState` 只包含 `authenticated`、用户资料和过期时间，不包含 Access Token 或 Refresh Token。登录成功后 Token 只保留在 Rust Core 和系统凭据库中。

图片压缩 Command 把阻塞工作交给 worker，并通过 `Channel<CompressionProgress>` 依次发送每张图片的 `processing` 和终态事件。`ImageService` 串行调度批次，`LocalImageProcessor` 使用 `image` 重编码 JPEG、使用 `oxipng` 无损优化 PNG。质量参数范围为 1–100 且只影响 JPEG。

## 稳定错误协议

可失败命令返回经过脱敏的结构：

```json
{
  "code": "database_unavailable",
  "message": "本地数据服务暂时不可用，请重试。"
}
```

当前稳定错误码：

| Code | 场景 |
|---|---|
| `auth_failed` | 认证服务拒绝手机号或密码，消息来自服务端 `errmsg` |
| `auth_unavailable` | 认证服务不可达、超时或返回无法解析的数据 |
| `database_unavailable` | SQLite 或本地数据服务不可用 |
| `config_load_failed` | 配置读取或恢复失败 |
| `config_save_failed` | 配置保存失败 |
| `config_reset_failed` | 默认配置恢复失败 |
| `task_not_found` | 指定任务不存在 |
| `task_queue_unavailable` | 任务队列已满、已关闭或正在退出 |
| `file_not_found` | 指定文件或目录不存在 |
| `file_access_denied` | OS 拒绝文件访问 |
| `file_too_large` | 文本文件超过读取上限 |
| `file_not_text` | 文件不是有效 UTF-8 文本 |
| `file_watch_unavailable` | 平台文件监听无法启动 |
| `file_watch_not_found` | 指定 watch ID 不存在 |
| `file_operation_failed` | 其他文件操作失败 |
| `image_decoding_failed` | PNG/JPEG 内容无法解码 |
| `image_encoding_failed` | JPEG 重编码或 PNG 优化失败 |
| `image_format_unsupported` | 输入不是 PNG/JPEG |
| `image_output_failed` | 输出目录不可写或同名文件已存在 |
| `image_operation_failed` | 其他图片处理失败 |
| `plugin_not_found` | 指定插件不存在 |
| `plugin_already_installed` | 相同插件 ID 已安装 |
| `plugin_invalid_package` | Manifest、模块、ABI 或沙箱校验失败 |
| `plugin_conflict` | 插件状态不允许当前操作 |
| `plugin_execution_failed` | 命令或生命周期 hook trap/失败 |
| `plugin_operation_failed` | 其他插件存储操作失败 |
| `update_not_configured` | 当前构建没有内嵌更新公钥 |
| `update_busy` | 另一个更新检查或安装正在运行 |
| `update_not_available` | 没有经过检查的待安装更新 |
| `update_check_failed` | 更新检查失败 |
| `update_install_failed` | 下载、签名验证或安装失败 |
| `update_operation_failed` | 其他内部更新服务错误 |
| `validation_failed` | 输入未通过验证 |
| `unknown_error` | 前端收到未知或不可信错误载荷 |

## 信息脱敏

Rust 端把原始 `AppError`、TaskManager、文件服务、插件运行时和更新后端诊断写入本地日志。发送给 WebView 的 `IpcError` 只包含稳定错误码与面向用户的消息；其中 `auth_failed` 使用认证服务的 `errmsg`，认证服务不可达或响应无法解析时则使用固定脱敏消息。错误载荷不包含：

- 手机号、密码、Access Token 或 Refresh Token；
- 本地文件路径；
- 图片编解码器、输出路径或 OS 文件错误；
- SQL 或数据库内部文本；
- Rust 类型和堆栈细节；
- 配置原始内容；
- worker panic 或任务内部错误细节；
- Manifest 解析位置、WASM 编译详情或 trap 文本；
- 更新 endpoint、签名、清单、请求头和安装器内部错误。

前端只接受白名单中的错误码。未知对象、原生 `Error` 或空消息都会归一化为：

```json
{
  "code": "unknown_error",
  "message": "操作失败，请重试。"
}
```

## 前端使用

认证：

```ts
const response = await login({
  username: phone,
  password,
});
const restored = await getAuthState();
await logout();
```

插件：

```ts
const installed = await installPlugin({ manifestPath });
await enablePlugin(installed.manifest.id);
const result = await executePluginCommand({
  pluginId: installed.manifest.id,
  command: "hello",
});
```

更新：

```ts
const update = await checkForUpdates();
if (update) {
  await installUpdate({
    restart: true,
    onProgress: (event) => console.log(event),
  });
}
```

所有原始 `invoke` 调用集中在 `src/services/tauri.ts`。认证、配置、任务、文件、图片、插件和更新均通过 `src/services/` 中的类型安全封装调用。纯错误映射位于 `src/services/tauri-errors.ts`。

## 测试

CI 同时执行：

- Rust：验证认证成功码、会话持久化/恢复/登出、登录/登出事件、错误映射、错误脱敏、参数边界、服务生命周期、图片请求/串行进度/JPEG 压缩与二次解码/输出不覆盖、插件 ABI、宿主 import 拒绝、资源限制、fuel trap、持久化恢复、更新串行化、可用事件和进度协议。
- TypeScript：验证已知认证/配置/任务/文件/图片/插件/更新错误保留、图片请求 envelope 与 Channel 回调、未知错误脱敏、空消息拒绝，并执行完整前端构建。
