# pjman 实现计划（v2，已按 SeaORM 与最新决策修订）

## Context

pjman 是一个管理本地 workspace(ws)/project(pj) 目录的 Rust CLI：通过 key-value tag 打破树状文件系统的单一分类限制，支持创建、浏览、删除、移动、更新、跳转工作区与项目。当前仓库仅有需求文档 [pjman.md](file:///Users/monica/Development/workspace/languages/rust/pjman/pjman.md)，从零搭建。

本计划相对 v1 的变化：

1. **持久层 SQLx → SeaORM 2.0**：用 ORM 实体与查询构造器解耦数据库方言，默认 SQLite，通过 cargo feature 保留 MySQL/MariaDB/PostgreSQL 接入能力，避免手写同态 SQL。
2. **实施范围 = 阶段 0 + 阶段 1**：地基 + tags/ws/pj 核心 CRUD、`--select`、jump、shell 集成与补全。backup/restore（阶段 2）、api-server（阶段 3）本次不实现，仅在架构边界上预留。
3. **初始虚拟 ws 固定命名 `dvws`（DefaultVirtualWorkspace）**：内置哨兵、不可 rename/remove、不支持用户自建虚拟 ws（无 `--virtual`）。
4. **删除策略**：`ws rm`/`pj rm` 默认仅注销 DB 记录，`--purge` 才删除磁盘目录。
5. **tag 值内含逗号**：同时支持反斜杠转义（`Authors='Matt Long\,Taylor Swift'`）与重复参数（多次 `--tags Authors=...` 取并集）。

环境：rustc 1.99.1，edition 2024；macOS/Linux tier1（amd64/arm64），Windows 次之。

## 一、技术选型与依赖

`Cargo.toml` 关键依赖：

```toml
[package]
name = "pjman"
edition = "2024"
rust-version = "1.85"

[features]
# 默认只启用 SQLite；外部 DB 驱动按需开启
db-mysql    = ["sea-orm/sqlx-mysql"]
db-postgres = ["sea-orm/sqlx-postgres"]

[dependencies]
clap            = { version = "4.6", features = ["derive"] }
clap_complete   = "4.6"
tokio           = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "sync", "process"] }
sea-orm         = { version = "2", features = ["macros", "sqlx-sqlite", "runtime-tokio-rustls", "with-chrono", "with-uuid"] }
sea-orm-migration = { version = "2", features = ["sqlx-sqlite", "runtime-tokio-rustls"] }
uuid            = { version = "1", features = ["v7"] }
chrono          = { version = "0.4", default-features = false, features = ["clock"] }
serde           = { version = "1", features = ["derive"] }
serde_json      = "1"
thiserror       = "2"
anyhow          = "1"
dirs            = "6"
dialoguer       = "0.12"
comfy-table     = "7"
tracing         = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

要点：

- SeaORM 查询均为运行时构造，**无编译期数据库依赖**；底层经 SQLx 但不直接使用 SQLx API。
- 多 DB 解耦落点：① schema 用 `sea-orm-migration` 的 `SchemaManager` 可移植 DDL（不写方言 SQL）；② 业务查询只用 Entity/`sea_query::Condition`，禁止裸 SQL；③ SQLite 专属 PRAGMA 仅在后端为 Sqlite 时条件执行；④ 连接串按 scheme（`sqlite:`/`mysql:`/`postgres:`）建连，默认 `sqlite://$PJMAN_HOME/pjman.sqlite?mode=rwc`，可用 `--database-url` 或 `PJMAN_DATABASE_URL` 覆盖。
- 阶段 2/3 依赖（tar/flate2/zip/sevenz/axum 等）届时再加，本次不提前引入。

## 二、模块结构

```
pjman/
├── Cargo.toml
└── src/
    ├── main.rs                 # 极薄：tokio main → CLI 解析 → 分发
    ├── cli.rs                  # 全部 clap derive 命令树（含 alias、隐藏 --print/__complete）
    ├── config.rs               # PJMAN_HOME（测试可重定向）、DB URL 解析
    ├── error.rs                # AppError：NotFound/Ambiguous/TagParse/PathConflict...
    ├── app.rs                  # AppCtx { db, cfg }，服务编排入口
    ├── db/
    │   ├── mod.rs              # Database::connect、SQLite PRAGMA、Migrator::up
    │   └── meta.rs             # meta 表 KV 读写（default_ws_id）
    ├── entity/                 # SeaORM 实体（2.0 dense 格式）
    │   ├── mod.rs
    │   ├── workspace.rs  project.rs  tag.rs
    │   ├── workspace_tag.rs  project_tag.rs  meta.rs
    ├── migration/
    │   ├── mod.rs              # Migrator 注册
    │   └── m20261020_000001_init.rs   # 可移植建表 DDL + dvws 种子
    ├── domain/                 # 纯逻辑，优先单测
    │   ├── ids.rs              # Uuid::now_v7；DVWS_ID 固定常量
    │   ├── tags.rs             # tag 解析器、TagSet（纯函数）
    │   ├── selector.rs         # 匹配/交集/歧义候选
    │   └── paths.rs            # 绝对化、规范化、跨平台前缀包含判断
    ├── service/                # 业务操作（事务边界在此）
    │   ├── tags.rs  workspace.rs  project.rs
    ├── commands/
    │   ├── mod.rs              # run(Cmd, &AppCtx) 分发
    │   ├── tags_cmd.rs  workspace.rs  project.rs
    │   ├── init.rs  completion.rs  complete.rs
    ├── shell/init.rs           # bash/zsh/fish/PowerShell 集成模板
    └── output.rs               # 表格 / --json / (missing)、(virtual) 标记
tests/
├── common/mod.rs               # PJMAN_HOME=tempdir 辅助
├── tags_parse.rs  select.rs  ws_pj_crud.rs  shell_init.rs  completion_smoke.rs
```

## 三、数据库 Schema（migration 中用 SchemaManager 建表）

- `meta(key PK, value)`：存 `default_ws_id`。
- `workspaces(id UUID PK, name TEXT UNIQUE, root_path NULL, is_builtin BOOL, created_at, updated_at)`：
  - `root_path` 非空 = 实体 ws；为空 = 虚拟 ws（仅 dvws 一条）。
  - 种子：固定 id 常量 `DVWS_ID`、name=`dvws`、root_path=NULL、is_builtin=true。
- `projects(id UUID PK, workspace_id FK→workspaces, name, path 绝对路径 UNIQUE, created_at, updated_at, UNIQUE(workspace_id,name))`。
- `tags(id UUID PK, tag_key, tag_value, created_at, UNIQUE(tag_key,tag_value))`：全局可用 tag 字典；实体打 tag 时自动 upsert。
- `workspace_tags(workspace_id FK CASCADE, tag_id FK, is_name BOOL, created_at, PK(workspace_id,tag_id))`。
- `project_tags(project_id FK CASCADE, tag_id FK, is_name BOOL, created_at, PK(project_id,tag_id))`。
- 索引：`tags(tag_key,tag_value)`、两张关联表 `(tag_id)`、`projects(workspace_id)`。

初始化（首启事务内）：执行迁移 → 确保 dvws 存在。SQLite 连接 PRAGMA：`foreign_keys=ON`、`journal_mode=WAL`、`busy_timeout=5000`、`synchronous=NORMAL`。

**name 即 tag**：ws/pj 创建时自动插入 `(DefaultTags, <name>)` 关联并置 `is_name=true`，它是该实体 DefaultTags 的首值，用于精确唯一选定；rename 在同一事务内更新 name 列与该 tag value。

## 四、tag 语法与 select 语义

解析规则（`domain/tags.rs`，纯函数先行单测）：

1. 第一个 `=` 切 key/value；无 `=` 的裸值归 `DefaultTags`。
2. value 按逗号拆多值；支持 `\,`（字面逗号）、`\=`、`\\` 转义。
3. `--tags` 可重复，同 key 多次出现取并集（两种写法可混用）。
4. key 校验 `[A-Za-z_][A-Za-z0-9_.-]*`；value 允许空格。
5. 帮助文本改写需求示例，说明 shell 剥引号后值内逗号需用 `\,` 或重复参数。

实体 tag 更新（`up`）：`--set k=v`（覆盖该 key）、`--add k=v`、`--remove k[=v]`；`is_name` 的 DefaultTags 受保护，只能由 rename 改。

`--select k1=v1,v2 k2=v3`：

- key 内多值 = OR；不同 key = AND。
- 可移植实现（不写方言 HAVING）：按 key 分组用 `Condition` 查关联 tag 行 → 在 Rust 中按实体 id 分组求交集（本地数据量小，避免方言 SQL）。
- 命中 1 个：选中；0 个：报错并给同 key 近似值建议；多个：TTY 下 dialoguer 单选（展示 `name (path) [tag...]`），非 TTY 打印可复制候选命令行（追加 `DefaultTags=<ws-name>` 收敛歧义）；`__complete` 动态补全同样输出 `DefaultTags=<name>` 候选。

## 五、命令清单（本次范围）

```
pjman [--home <dir>] [--database-url <url>] [--json] [-y/--yes] [-v...]
  tg|tags
    a|add <tag>...                 # 注册全局可用 tag（实体打 tag 时也自动 upsert）
    rm|remove <key>[=<value>]... [--force]   # 默认被引用时拒绝
    ls|list [--key <k>] [--json]
  ws|workspace
    c|create [names...] [--path <parent=.>] [--tags ...] [--default]
              # 无 names：创建单个 ws-<uuidv7> 实体 ws 并设为默认
              # 多 names：在同一 parent 下批量创建
    a|add <path> [--name <n>] [--tags ...]   # 登记已有目录为实体 ws
    ls|list [--select ...] [--tag k=v...] [--json]   # dvws 标 (virtual)，断链标 (missing)
    up|update <name> [--add/--remove/--set ...]
    rn|rename <old> <new>          # dvws 禁止
    rm|remove <name> [--purge]     # 默认仅注销 DB；--purge 删目录（非空需确认）
    jp|jump <name-or-select> [--print]
    use <name>                     # 切换默认 ws（dvws 不可设为默认）
  pj|project
    c|create [name] (--workspace <ws> | --select ...) [--tags ...]
              # 无 name ⇒ pj-<uuidv7>；位置=<实体 ws.root>/<name>（扁平）
              # 无任何实体 ws 时报错并引导先 ws create（dvws 只承载 add）
    a|add <path> (--workspace|--select，缺省=dvws) [--name] [--tags ...]
    ls|list [--workspace|--select|--all] [--tag ...] [--json]
    up|update <name> [--scope <ws>] [--add/--remove/--set ...]
    rn|rename <name> <new> [--workspace <scope>]
    rm|remove <name> [--workspace <scope>] [--purge]
    mv|move <name> --to <ws> [--db-only]   # 默认连磁盘目录一起移动（同盘 rename/跨盘 copy+删）
    jp|jump <name> [--workspace|--select 限范围] [--print]
  init <bash|zsh|fish|powershell>     # 输出 shell 函数 + 拼接补全脚本
  completion <bash|zsh|fish|powershell|elvish>
  __complete ws|pj|tagkey|tagvalue <partial>...   # hidden：查 DB 的动态候选
```

目标 ws 解析顺序：`--workspace` > `--select` > `meta.default_ws_id` > 报错并列候选。pj 同名跨 ws 歧义时要求 `--workspace` 限定。

## 六、jump 与 shell 集成

子进程无法修改父 shell cwd，采用 zoxide 同款方案：`pjman init <shell>` 输出包装函数，只拦截 `ws|pj jump|jp`，执行 `cd "$(command pjman ... jump --print ...)"`，其余参数原样转发二进制；init 输出尾部拼接 `completion <shell>` 静态脚本，一次 eval 同时装好函数与补全。

- `jump --print` 只输出解析到的绝对路径；dvws 无目录时报明确错误并建议 jump 其下 pj；断链路径报错。
- 动态补全：优先 clap_complete 的 dynamic completion 为 `--workspace`/jump 位置参数注册 DB 候选；保底用隐藏 `__complete` + 手写 shell 钩子。
- 安装方式写入帮助：`eval "$(pjman init zsh)"` 加入 shell rc。

## 七、实施步骤

**阶段 0：地基**

1. `cargo init`，写 Cargo.toml（edition 2024、features、CI 友好配置）。
2. cli.rs 命令树骨架 + config（PJMAN_HOME）+ error + output（表格/JSON）。
3. db 建连（SQLite 默认 URL）、条件 PRAGMA、Migrator 注册、migration 建表与 dvws 种子。
4. entity 六个文件 + ids（uuidv7、DVWS_ID）。
5. 验证：`cargo build` 零 warning；`PJMAN_HOME=<tmp> pjman ws ls` 能看到 dvws。

**阶段 1：核心功能**

6. `domain/tags.rs` 解析器 + 单测（转义、多值、重复参数并集、裸值、非法 key）。
7. `domain/paths.rs`、`domain/selector.rs` + 单测（AND/OR、0/1/多命中、候选生成）。
8. service/tags + `tg add/rm/ls`。
9. service/workspace + ws 全部子命令（默认 ws、dvws 保护、断链标记、--purge）。
10. service/project + pj 全部子命令（扁平创建、外部 add 归属 dvws、move、rename 联动 name tag）。
11. jump --print + `init` 四 shell 模板 + 静态 completion + `__complete` 动态候选。
12. 集成测试与 CI（见验证节）。

## 八、验证

- 静态：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
- 集成测试（每个用例独立 `PJMAN_HOME=tempfile`）：
  - 空库 `ws ls` 仅有不可删的 dvws；dvws rename/remove 被拒。
  - bare `ws create` 生成 `ws-<uuidv7>` 且成为默认；批量 create、`ws add` 登记外部目录。
  - 无实体 ws 时 `pj c` 报错引导；`pj add` 默认挂 dvws；`pj c` 在 ws 内扁平建目录。
  - tag：转义逗号、重复 `--tags` 并集；`--select` 唯一/歧义/零命中；追加 `DefaultTags=<name>` 收敛。
  - rename 后 DefaultTags 首值联动；`rm` 默认保留目录、`--purge` 删除；`pj mv` 跨 ws 移动磁盘目录。
  - `jump --print` 输出正确绝对路径；断链报 (missing) 错误。
  - init 脚本经 `bash -n`/`zsh -n`/`fish --no-execute` 语法检查；`completion` 四 shell 可生成。
- CI 矩阵：ubuntu/macOS × amd64/arm64 + windows x64，跑 fmt/clippy/test（macOS/Linux 为 tier1 门禁，Windows 允许后续补绿）。

## 九、阶段 2/3 预留（本次不做）

- 阶段 2：`backup`/`restore` 命令；SQLite 一致性快照（SeaORM 后端连接上执行 VACUUM INTO）+ ws/pj 目录归档为 tar.gz（默认）/zip/7z，manifest.json + 校验和，staging 恢复、$HOME 前缀重映射、冲突策略。
- 阶段 3：`api-server`（axum），仅 GET/POST、loopback、token 鉴权，薄封装同一 service 层。
- 多 DB：阶段 1 已保证无方言 SQL/DDL；阶段 2 起在 CI 加 MySQL/PostgreSQL（db-mysql/db-postgres feature）冒烟。
