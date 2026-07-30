# ShenDesk Design Specification

## 1. 产品定位

ShenDesk（Shencom Desktop Platform）是深传科技桌面应用平台。

目标：为企业员工提供统一、安全、高效的桌面工具入口，降低日常工作中的重复操作成本。

核心方向：

- 工具管理
- 自动化脚本执行
- 企业内部效率提升
- 本地安全运行
- 可扩展插件生态

技术基线基于 Tauri 2、React、TypeScript、Rust 构建。当前仓库采用 React 19 + TypeScript、Vite、Tailwind CSS、shadcn/ui 与 Rust 技术栈。参考项目 README。 

## 2. 用户角色

### 普通员工

关注：

- 快速找到需要的工具
- 一键执行自动化任务
- 查看任务执行结果
- 减少复杂操作

### 管理员

关注：

- 工具分发
- 脚本权限控制
- 使用统计
- 企业配置管理

### 开发人员

关注：

- 创建插件
- 编写自动化能力
- 扩展平台能力

## 3. 产品设计原则

### 简单

员工不需要理解技术细节，只需要选择工具并执行。

### 安全

所有脚本和插件必须经过权限控制和隔离运行。

### 可扩展

通过插件系统支持未来能力扩展。

### 本地优先

优先保证离线可用，减少对网络依赖。

## 4. 核心功能模型

```
员工
 |
ShenDesk
 |
工具中心
 |
+------------+
| 内置工具   |
| 自动化脚本 |
| 插件能力   |
| AI助手     |
+------------+
```

## 5. 工具系统设计

每个工具由以下信息组成：

```json
{
  "name": "图片压缩",
  "category": "效率工具",
  "version": "1.0.0",
  "permissions": ["file.read", "file.write"]
}
```

工具生命周期：

```
安装
 |
启用
 |
运行
 |
更新
 |
卸载
```

## 6. 脚本系统设计

目标：让员工执行标准化自动化流程。

支持：

- Shell Script
- PowerShell
- Python Script
- WASM Script

执行模型：

```
用户操作
 |
TaskManager
 |
Script Runner
 |
执行环境
 |
结果反馈
```

## 7. 权限模型

所有工具声明权限：

```
File Access
Network Access
System Command
Clipboard
Credential
```

原则：

- 默认最小权限
- 用户可确认授权
- 高风险操作必须提示

## 8. 任务系统

所有耗时操作进入任务管理。

状态：

```
pending
running
success
failed
cancelled
```

支持：

- 进度展示
- 后台执行
- 取消任务
- 错误日志

## 9. 插件系统

插件用于扩展 ShenDesk 能力。

架构：

```
Plugin
 |
Plugin API
 |
Rust Core
```

设计目标：

- 沙箱运行
- 跨平台
- 版本管理
- 安全隔离

当前插件体系采用 Manifest + Wasmtime 沙箱方案。

## 10. UI 设计规范

### 导航结构

```
首页
 |
工具中心
 |
任务记录
 |
插件管理
 |
设置
```

### 设计风格

- 简洁企业风
- 高信息密度
- 明确操作反馈
- 支持深色模式

## 11. 数据设计

本地数据：

- 用户配置
- 工具配置
- 执行历史
- 日志记录

存储：

```
SQLite
+
SQLx
```

## 12. 安全设计

要求：

- 敏感信息使用系统密钥存储
- 插件隔离运行
- 操作日志记录
- 错误信息脱敏

## 13. 企业化能力规划

未来支持：

- 企业工具市场
- 统一配置中心
- 员工权限管理
- 使用分析
- AI Agent
- 云端同步

## 14. 版本规划

### Phase 1

基础桌面平台：

- 桌面框架
- 工具入口
- 配置管理
- 日志系统

### Phase 2

效率平台：

- TaskManager
- 脚本系统
- 文件能力
- 用户体系

### Phase 3

平台生态：

- Plugin System
- AI Agent
- 企业能力
- 插件市场

## 15. 长期愿景

ShenDesk 不只是一个桌面应用，而是企业员工数字化工作入口。

未来形态：

```
ShenDesk
 |
 +-- Tools
 |
 +-- Scripts
 |
 +-- AI Agent
 |
 +-- Plugins
 |
 +-- Cloud
```
