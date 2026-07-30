# ShenDesk DESIGN.md

> ShenDesk 官方 UI/UX 设计系统规范
>
> 用于指导 AI Agent、开发人员和设计人员创建一致的桌面应用体验。

---

# 1. Design Identity

## Product

ShenDesk（Shencom Desktop Platform）

定位：企业员工数字工作入口。

不是传统工具集合，而是帮助员工发现能力、执行任务、提升效率的桌面工作平台。

## Design Keywords

```
Simple
Efficient
Trusted
Professional
Intelligent
```

## Experience Goal

用户打开 ShenDesk 后：

1. 快速找到需要的工具
2. 理解工具能解决什么问题
3. 安全执行任务
4. 清晰获得结果

---

# 2. Design Principles

## 2.1 Simple First

降低认知成本。

规则：

- 一个页面只解决一个核心任务
- 默认提供最佳选择
- 高级能力隐藏
- 避免技术语言

## 2.2 Trust By Design

企业工具必须让用户放心。

任何涉及：

- 文件
- 网络
- 系统命令
- 数据修改
- 权限

必须明确展示原因和影响。

## 2.3 Efficient Workflow

减少重复操作。

优先设计：

- 快捷入口
- 最近使用
- 一键执行
- 键盘操作

## 2.4 Consistent System

所有功能遵循统一设计语言。

禁止：

- 自定义组件风格
- 不一致交互
- 不统一状态反馈

---

# 3. Visual Language

## Overall Style

现代企业桌面应用。

参考方向：

- Linear
- Raycast
- VS Code
- Notion
- Apple macOS Apps

特点：

- 大量留白
- 清晰层级
- 精准间距
- 高信息密度

---

# 4. Layout System

标准窗口：

```
+--------------------------------+
| Header                         |
+------+-------------------------+
| Side | Main Content            |
| Bar  |                         |
|      |                         |
+------+-------------------------+
```

## Sidebar

负责：

- 主导航
- 工具分类
- 用户入口

要求：

- 固定宽度
- 图标 + 文本
- 当前状态明显

## Content

负责：

- 工作区域
- 工具执行
- 数据展示

---

# 5. Navigation

一级导航：

```
Home
Tools
Tasks
Plugins
Settings
```

规则：

- 不超过 6 个入口
- 高频功能靠前
- 设置永远最后

---

# 6. Component System

技术基础：

- React
- Tailwind CSS
- shadcn/ui

所有组件必须复用设计系统。

核心组件：

- Button
- Card
- Dialog
- Input
- Table
- Toast
- Progress
- Badge

---

# 7. Tool Center Design

工具是 ShenDesk 的核心。

## Tool Card

结构：

```
+----------------+
| Icon           |
| Name           |
| Description    |
| Permission     |
| Action Button  |
+----------------+
```

必须展示：

- 工具名称
- 功能说明
- 权限需求
- 执行动作

禁止只显示技术名称。

---

# 8. Task Experience

所有耗时任务进入 Task Center。

状态：

```
Pending
Running
Success
Failed
Cancelled
```

必须提供：

- 进度
- 日志
- 错误原因
- 重试能力

---

# 9. Permission Experience

权限不是隐藏信息。

展示：

```
Tool
 |
Required Permission
 |
User Confirmation
 |
Execute
```

原则：

最小权限 + 清晰授权。

---

# 10. Theme System

支持：

- Light Mode
- Dark Mode

设计要求：

- 保持信息层级一致
- 不依赖颜色表达状态
- 保证可访问性

---

# 11. Interaction Rules

## Feedback

所有操作必须反馈：

Before:

- 即将执行什么
- 需要什么权限

During:

- 当前进度
- 当前状态

After:

- 结果
- 下一步

## Animation

原则：

- 快
- 少
- 有意义

禁止装饰动画。

---

# 12. Empty & Error States

## Empty State

必须包含：

- 当前状态
- 为什么为空
- 下一步操作

## Error State

必须回答：

1. 发生什么
2. 为什么发生
3. 如何解决

禁止：

```
Error 500
```

推荐：

```
文件访问失败，请检查权限后重试
```

---

# 13. Plugin UI

插件展示：

- Logo
- Name
- Description
- Version
- Author
- Permissions

安装流程：

```
View
 ↓
Review Permission
 ↓
Install
 ↓
Enable
```

---

# 14. Desktop Experience

平台：

- macOS
- Windows

必须支持：

- 键盘快捷键
- 系统通知
- 原生窗口行为
- 系统主题同步

---

# 15. AI Generated UI Rules

AI 创建页面时必须遵循：

- 使用现有组件
- 使用设计 Token
- 保持布局一致
- 优先复用已有模式

禁止：

- 创建新的视觉语言
- 随意增加颜色
- 自定义交互模式

---

# 16. Design Review Checklist

上线前检查：

- 用户 3 秒理解功能
- 核心任务 3 步完成
- 权限透明
- 状态明确
- 错误可恢复
- 深色模式正常

---

# 17. Long Term Vision

ShenDesk 最终成为：

```
Employee
   |
ShenDesk
   |
Tools
Scripts
AI Agent
Plugins
Cloud
```

设计目标：

让每个员工拥有自己的智能工作入口。
