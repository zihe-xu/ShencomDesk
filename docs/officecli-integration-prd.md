# OfficeCLI 内置能力 PRD

## 文档信息

| 项目 | 内容 |
| --- | --- |
| 产品 | ShenDesk |
| 能力 | Office 文档引擎内置一期 |
| 状态 | 内部交付完成 |
| 目标平台 | macOS Apple Silicon、macOS Intel、Windows x64 |
| 上游基线 | OfficeCLI `v1.0.143`，commit `fd4adab4dbe3283b62e3edcfad124dd648fa74bc` |
| 技术设计 | [`officecli-integration.md`](officecli-integration.md) |

## 背景

ShenDesk 当前具备本地文件、图片压缩、任务管理和 WASM 插件等平台能力，但没有统一的 Office 文档处理引擎。用户若需要读取、创建或修改 Word、Excel、PowerPoint 文件，仍需依赖 Microsoft Office、不同格式的第三方库或外部服务。

OfficeCLI 提供不依赖 Microsoft Office 的 `.docx`、`.xlsx`、`.pptx` 创建、读取、修改和渲染能力。将其以固定源码版本编译并随 ShenDesk 分发，可以为后续文档自动化和 AI 工具调用提供统一的本地执行基础。

## 产品目标

一期面向内部用户交付一个可测试、默认本地执行的 Office 文档基础能力：

1. ShenDesk 安装后即可使用，不要求用户安装 Office、.NET 或 OfficeCLI。
2. 支持创建、打开、读取、批量修改和保存 `.docx`、`.xlsx`、`.pptx`。
3. 支持将文档渲染为 PNG 预览，避免在主 WebView 中执行文档生成的 HTML。
4. 所有 OfficeCLI 调用由 Rust Core 管理，WebView 不获得任意进程执行权限。
5. OfficeCLI 版本、源码来源和升级节奏由 ShenDesk 控制，不允许运行时自行更新。
6. 在 ShenDesk 内部支持的 macOS、Windows 目标上通过最终构建产物内 smoke test。

## 非目标

一期不包含：

- 实现完整的 Word、Excel 或 PowerPoint 可视化编辑器。
- 提供自然语言生成文档的完整产品交互。
- 将 OfficeCLI 改造成现有 WASM 插件。
- 把 C#/.NET 运行时嵌入 ShenDesk Rust 主进程。
- 支持 OfficeCLI 的 `watch` 本地 HTTP 预览。
- 支持用户安装任意版本的 OfficeCLI 或执行任意 CLI 参数。
- 自动跟随 OfficeCLI 每个上游版本发布。
- 面向外部用户的 Developer ID 签名、公证或无警告安装体验。

## 目标用户与场景

### 目标用户

- 需要在 ShenDesk 中处理 Office 文件的业务人员。
- 后续需要调用 Office 工具的 ShenDesk 自动化或 AI 功能。
- 需要在未安装 Microsoft Office 的设备上生成或检查文档的用户。

### 核心场景

1. 用户选择一个 Office 文件，ShenDesk 获取其结构化内容和基础元数据。
2. 上层功能提交一组明确的结构化修改，ShenDesk 原子保存结果文件。
3. 用户或上层功能创建空白 Word、Excel、PowerPoint 文件并写入内容。
4. ShenDesk 将指定页、工作表或幻灯片渲染为 PNG，用于预览或后续视觉检查。
5. 操作失败、超时或被取消时，原文件不被半写入，用户得到稳定且不泄露本地路径的错误提示。

## 用户故事

### US-01 开箱即用

作为 ShenDesk 用户，我希望安装应用后直接使用 Office 文档能力，不需要额外安装 OfficeCLI 或 .NET。

验收条件：

- 内部构建产物包含与当前平台和架构匹配的 OfficeCLI sidecar。
- 应用可以返回内置 OfficeCLI 版本和可用状态。
- 缺失、损坏或架构不匹配时返回稳定错误，不尝试在线安装。

### US-02 创建 Office 文档

作为用户，我希望创建 `.docx`、`.xlsx` 或 `.pptx` 文件，并通过结构化操作写入内容。

验收条件：

- 支持三种目标扩展名。
- 目标文件已存在时默认不覆盖。
- 创建成功后可以再次读取并获得预期内容。

### US-03 读取文档结构

作为上层功能，我希望读取文档结构化内容，以便理解段落、单元格、工作表、幻灯片和形状。

验收条件：

- 输入仅接受由可信系统文件选择流程得到的绝对路径。
- 返回结构化 JSON，不把 OfficeCLI 原始进程错误直接暴露给 WebView。
- 支持 `.docx`、`.xlsx` 和 `.pptx` 的代表性样本。

### US-04 批量修改并保存

作为上层功能，我希望一次提交多个修改，减少反复启动进程并保持文档修改顺序。

验收条件：

- 接受白名单内的结构化 batch 操作。
- 同一文档的修改串行执行。
- 成功后关闭 resident 并将结果刷新到磁盘。
- 失败时不返回可能含有本地路径或文档内容的原始 stderr。

### US-05 PNG 预览

作为用户或上层功能，我希望查看文档渲染结果，而不依赖本机 Office。

验收条件：

- 输出 PNG 文件列表及页码或幻灯片编号。
- PNG 通过现有可信本地资源方式展示。
- 一期不在主 WebView 中加载 OfficeCLI 生成的 HTML 或启动 `watch` 服务。

### US-06 取消与退出清理

作为用户，我希望长时间操作可以取消，并且退出 ShenDesk 后不留下由 ShenDesk 启动的 OfficeCLI resident。

验收条件：

- 长任务接入现有 TaskManager 的取消语义。
- 取消后停止后续操作并执行 best-effort 文档关闭。
- 应用退出时关闭仍由本进程管理的文档会话。

## 功能范围

### P0：发布与运行基础

- 固定 OfficeCLI 源码 tag、commit 和源码归档 SHA-256。
- CI 使用固定 .NET 10 SDK 从源码构建自包含单文件 sidecar。
- 支持 macOS arm64、macOS x64、Windows x64。
- sidecar 随合并后内部构建产物分发；未来公开发布仍使用独立签名流程。
- Rust Core 提供版本探测、进程调用、超时、输出上限和关闭能力。
- 所有调用设置 `OFFICECLI_SKIP_UPDATE=1`。
- 分发 Apache-2.0 `LICENSE`、`NOTICE` 和第三方声明。

### P0：文档操作

- `health/version`：确认内置引擎可运行。
- `create`：创建三种 Office 文档。
- `inspect`：返回文档结构化内容。
- `batch`：执行受支持的结构化修改。
- `render`：生成 PNG 预览。
- `close`：刷新并关闭文档 resident。

### P1：上层产品接入

- 将 Office 文档能力接入后续自动化或 AI 工具调用。
- 提供文档操作进度和结果展示。
- 根据真实使用数据补充更细粒度的操作白名单。

## 交互与状态要求

一期不规定完整编辑器界面，但所有调用方必须能够区分以下状态：

- 引擎可用或不可用；
- 操作等待、运行、成功、失败或取消；
- 文档不存在、格式不支持、文件被占用、输出冲突、操作超时；
- 预览生成成功但部分页面失败。

用户可见消息使用固定中文提示；详细诊断只进入本地日志，并对路径和文档内容脱敏。

## 非功能要求

### 安全

- WebView 不获得 shell、进程或通用文件系统权限。
- Rust Core 只执行应用包内解析出的 OfficeCLI 固定路径，不从 `PATH` 查找。
- 不允许传入可执行路径、环境变量或任意 CLI 子命令。
- 输入和输出必须为绝对路径，并通过业务命令的路径规则校验。
- 默认不覆盖原文件或已有输出文件。
- 禁用 OfficeCLI 自更新、skill 安装和在线安装。
- 不在主 WebView 加载文档生成的 HTML。

### 可靠性

- 单个进程调用具有固定超时和输出大小上限。
- OfficeCLI 非零退出、崩溃或 JSON 无法解析时映射为稳定错误。
- 同一文档不允许并发写入。
- 应用异常结束后，下次启动能够识别并清理本应用遗留的临时文件；不终止其他程序创建的 OfficeCLI resident。

### 发布质量

- 每个平台从最终安装包或 `.app` 中执行 smoke test，而不是只测试构建目录中的裸二进制。
- macOS arm64、macOS x64 的内部 `.app` 均能实际启动 OfficeCLI 并完成文档回归。
- Windows x64 安装后能够执行版本探测和代表性文档操作。
- 内部 macOS 产物不承诺 Developer ID 签名、公证或 Gatekeeper 无警告安装；不得要求用户全局关闭 Gatekeeper。

### 可维护性

- 上游版本只通过版本清单升级。
- 上游源码不直接复制进 ShenDesk，初期不维护功能性 fork。
- 每次升级必须运行文档回归样本和跨平台构建验证。

## 成功指标

一期完成时使用交付指标验收：

- 三个内部目标平台的最终产物 smoke test 全部通过。
- Word、Excel、PowerPoint 各至少一个创建、读取、修改、关闭回归样本通过。
- IPC 测试证明前端不能指定二进制路径或任意命令。
- OfficeCLI 运行时不会发起自更新，也不会修改应用包内文件。

一期上线后再收集操作成功率、耗时和失败类型；本期不预设业务使用量指标。

## 发布门槛

以下条件全部满足后即可完成内部一期交付：

1. 固定源码与依赖构建链已进入 CI。
2. Rust Core、IPC、前端 service 和稳定错误测试通过。
3. 三种文档格式的回归样本通过。
4. macOS arm64、macOS x64 最终 `.app` 完成 OfficeCLI 实际执行和文档 smoke。
5. Windows 安装包完成安装后 smoke test。
6. 第三方许可证随安装包分发并可在文档中查阅。
7. 安全评审确认未向 WebView 暴露通用进程执行能力。

面向外部用户发布时，仍必须另行完成 Developer ID 签名、Apple 公证、Gatekeeper 验证和正式更新资产验收；这些条件不属于内部一期关闭门槛。

## 后续方向

- 接入自然语言文档生成与修改流程。
- 增加模板填充和批量报表能力。
- 根据安全评估决定是否提供隔离 WebView 的交互式 HTML 预览。
- 增加 Linux 正式发布目标后，再启用对应 OfficeCLI RID。
