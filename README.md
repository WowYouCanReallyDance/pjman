# pjman
> ⚠️ Early development, no stable release yet

[中文](./README-zh.md) | [日本語](./README-ja.md) | [한국어](./README-ko.md)

This project, named **pjman**, is currently in early-stage development.
It is a Rust-based CLI tool for local project directory management, helping developers organize messy, tangled and hard-to-classify project folders.

On the roadmap: before the first public release, service mode will be implemented.
It will expose database connections, an OpenAPI for AI agents and external services, as well as MCP artifacts callable by AI.

Many developers face this pain point: dozens or hundreds of local projects on Mac, Windows or Linux are either piled into one folder or scattered across disks. Locating projects is hard, and frequent `ls` / `cd` navigation is tedious.

pjman addresses exactly this pain. You can create workspaces and register your projects.
- `pjman jump kubernetes`: jump to the target project directory
- `pjman jump --select ProgrammingLanguage=Java KlassOf=OpenSource License=MIT`: jump to projects matching all tags, or select from matched results

Most importantly, pjman brings this management capability to AI coding agents, allowing them to manage numerous ongoing local projects naturally.
