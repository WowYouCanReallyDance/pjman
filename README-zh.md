# pjman
> ⚠️ 早期开发阶段，暂无稳定版本

[English](./README.md) | [日本語](./README-ja.md) | [한국어](./README-ko.md)

pjman 是一个尚处于早期开发阶段、使用 Rust 编写的本地项目目录管理 CLI 工具，帮助开发者整理杂乱、难以归类的本地项目文件夹。

项目规划：在首个公开版本发布前，将实现服务模式。
该模式对外提供数据库连接、可供 AI Agent 和外部服务调用的 OpenAPI，同时提供可被 AI 调用的 MCP 能力。

很多开发者都会遇到这个痛点：在 Mac、Windows、Linux 上，数十乃至上百个本地项目要么全部堆在同一个文件夹，要么散乱分布在磁盘各处。查找项目十分麻烦，反复执行 `ls`、`cd` 切换目录的体验很差。

pjman 正是为解决这个问题而生。你可以创建工作区并录入项目：
- `pjman jump kubernetes`：直接跳转目标项目目录
- `pjman jump --select ProgrammingLanguage=Java KlassOf=OpenSource License=MIT`：跳转至满足全部标签条件的项目，多匹配项时提供选择列表

最重要的是，pjman 为 AI 编码智能体提供这套管理能力，让 AI 也可以顺畅管理大量正在开发的本地项目。
