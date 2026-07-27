# Plugin System

ShenDesk Phase 3 的首个平台能力是一套本地优先、默认拒绝宿主访问的 WASM 插件内核。当前实现负责插件包校验、安装、启停、命令执行、卸载、事件发布与 Tauri IPC；在线市场、签名信任链、远程下载和企业策略将在独立能力中演进。

## 插件包

一个本地插件包是同一目录中的两个文件：

```text
hello-plugin/
├── plugin.json
└── hello.wasm
```

`plugin.json` 示例：

```json
{
  "apiVersion": 1,
  "id": "com.shencom.hello",
  "name": "Hello Plugin",
  "version": "1.0.0",
  "entrypoint": "hello.wasm",
  "description": "Minimal ShenDesk plugin",
  "commands": [
    {
      "name": "hello",
      "export": "hello",
      "description": "Return a plugin-defined status code"
    }
  ]
}
```

约束：

- `apiVersion` 当前必须为 `1`。
- `id` 使用反向域名风格的小写标识，仅允许 ASCII 小写字母、数字、点和连字符，并且至少包含一个点。
- `entrypoint` 必须是插件目录下单层、相对的 `.wasm` 文件名，禁止绝对路径和 `..`。
- 命令名称和导出名必须唯一。
- Manifest 最大 64 KiB，WASM 模块最大 16 MiB，每个插件最多声明 64 个命令。
- 安装文件必须是 binary WebAssembly；WAT 文本只用于开发示例与测试，不作为可安装格式。
- 安装时复制到 ShenDesk 管理目录，后续执行不再依赖来源目录。

## ABI v1

所有导出均采用无参数、返回 `i32` 的最小 ABI：

```wat
(module
  (func (export "shendesk_plugin_api_version") (result i32)
    i32.const 1)

  (func (export "shendesk_on_enable") (result i32)
    i32.const 0)

  (func (export "shendesk_on_disable") (result i32)
    i32.const 0)

  (func (export "hello") (result i32)
    i32.const 7)
)
```

- `shendesk_plugin_api_version` 必须存在并返回 `1`。
- `shendesk_on_enable` 和 `shendesk_on_disable` 可选；返回 `0` 表示成功。
- Manifest 声明的命令导出必须存在且签名匹配。
- 命令返回值由插件定义，宿主原样返回给调用方。

ABI v1 故意不接收字符串、文件或网络参数。后续扩展必须通过新的 API 版本和显式能力授权实现，不能静默扩大旧插件权限。

## 生命周期

```text
Install (disabled)
   ↓
Enable
   ↓
Execute commands
   ↓
Disable
   ↓
Uninstall
```

- 安装前完成 Manifest、模块导入、API 版本与导出签名校验。
- 插件安装后默认 `disabled`。
- 启用时先运行可选 enable hook，成功后再持久化状态。
- 禁用时先运行可选 disable hook，成功后再持久化状态。
- 卸载已启用插件时先禁用。
- 应用退出时对仍启用的插件执行 best-effort disable hook，但保留 enabled 偏好以便下次启动恢复；单个插件失败不会阻止其他资源关闭。
- 生命周期和命令调用由进程内互斥锁串行化，避免启停与执行竞态。

插件状态存放在应用数据目录的 `plugins/<plugin-id>/state.json`，插件 Manifest 与模块一同保存在同一受管目录。启动时损坏或不完整的插件目录会被跳过并记录脱敏诊断，不进入运行态。

## 沙箱边界

当前运行时使用 Wasmtime，并采取默认拒绝策略：

- 不启用 WASI。
- 不提供任何宿主 import；带 import 的模块在安装时直接拒绝。
- 每次验证或调用都创建独立 `Store` 和实例，不在插件间共享线性内存。
- 每次调用最多使用 10,000,000 fuel；无限循环会因 fuel 耗尽而 trap。
- 线性内存上限 64 MiB，表元素上限 10,000；单次 Store 最多一个实例、一个内存和一个表。
- WASM 栈上限 512 KiB，资源增长失败时 trap。
- 用户可见 IPC 错误只返回稳定代码和固定消息；路径、解析器细节与运行时 trap 文本只进入本地日志。

这些限制是防御边界，而不是对恶意本机代码的完整信任替代。未来市场安装必须再叠加发布者签名、哈希验证、权限声明与撤销机制。

## Tauri IPC

| Command | 输入 | 输出 |
| --- | --- | --- |
| `install_plugin` | `{ request: { manifestPath } }` | `PluginSnapshot` |
| `list_plugins` | 无 | `PluginSnapshot[]` |
| `get_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `enable_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `disable_plugin` | `{ pluginId }` | `PluginSnapshot` |
| `execute_plugin_command` | `{ request: { pluginId, command } }` | `PluginExecution` |
| `uninstall_plugin` | `{ pluginId }` | 插件 ID |

React 侧统一通过 `apps/desktop/src/services/plugins.ts` 调用，不在组件中直接使用原始 `invoke`。

## EventBus

生命周期通过共享 EventBus 发布：

- `plugin_installed`
- `plugin_enabled`
- `plugin_disabled`
- `plugin_executed`
- `plugin_removed`

事件只包含 Manifest、状态、命令名、返回码和 fuel 消耗等领域数据，不包含来源路径或 WASM 字节。

## 当前非目标

- JavaScript/原生动态库插件。
- WASI 文件系统、网络、环境变量或进程权限。
- 插件间调用和共享状态。
- 后台常驻插件任务。
- 在线插件市场、自动更新、依赖解析、发布者签名与企业策略。
