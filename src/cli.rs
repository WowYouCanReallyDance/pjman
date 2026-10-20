use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

/// 管理本地工作空间（workspace）与项目目录（project）
#[derive(Parser, Debug)]
#[command(name = "pjman", version, about, long_about = None)]
pub struct Cli {
    /// 数据目录（默认 ~/.pjman，或环境变量 PJMAN_HOME）
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,

    /// 数据库连接串（默认 sqlite://<home>/pjman.sqlite，或环境变量 PJMAN_DATABASE_URL）
    #[arg(long, global = true, value_name = "URL")]
    pub database_url: Option<String>,

    /// 以 JSON 输出结果
    #[arg(long, global = true)]
    pub json: bool,

    /// 对确认提示一律回答 yes
    #[arg(short, long, global = true)]
    pub yes: bool,

    /// 增加日志详细程度（可重复）
    #[arg(short = 'v', long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 全局可用 tag 管理
    #[command(visible_alias = "tg", subcommand)]
    Tags(TagsCmd),

    /// 工作空间管理
    #[command(visible_alias = "ws", subcommand)]
    Workspace(WorkspaceCmd),

    /// 项目管理
    #[command(visible_alias = "pj", subcommand)]
    Project(ProjectCmd),

    /// 输出 shell 集成脚本（jump 函数 + 补全）
    Init {
        /// 目标 shell
        shell: InitShell,
    },

    /// 输出静态补全脚本
    Completion {
        /// 目标 shell
        shell: Shell,
    },

    /// 动态补全候选查询（shell 钩子内部调用）
    #[command(hide = true, name = "__complete")]
    Complete {
        /// 补全对象类型
        kind: CompleteKind,
        /// 当前已输入的参数
        partial: Vec<String>,
    },
}

// ---------- 共享参数 ----------

/// tag 参数：`--tags k=v`、`--tags k=v1,v2`、裸值 `--tags OpenSrc`
#[derive(Args, Debug, Clone, Default)]
pub struct TagArgs {
    #[arg(long = "tags", value_name = "TAG", num_args = 1..)]
    pub tags: Vec<String>,
}

/// 按 tag 选择唯一实体的参数
#[derive(Args, Debug, Clone, Default)]
pub struct SelectArgs {
    /// 直接指定 workspace 名称
    #[arg(long, value_name = "WS")]
    pub workspace: Option<String>,

    /// 按 tag 选择，如 `--select License=MIT Language=Java,JavaScript`
    #[arg(long = "select", value_name = "SELECTOR", num_args = 1..)]
    pub select: Vec<String>,
}

/// tag 变更参数（update 子命令）
#[derive(Args, Debug, Clone, Default)]
pub struct TagMutationArgs {
    /// 覆盖某个 key 的全部值
    #[arg(long = "set", value_name = "TAG", num_args = 1..)]
    pub set: Vec<String>,

    /// 追加 tag
    #[arg(long = "add", value_name = "TAG", num_args = 1..)]
    pub add: Vec<String>,

    /// 移除 tag（`k` 删整个 key，`k=v` 删单个值）
    #[arg(long = "remove", value_name = "TAG", num_args = 1..)]
    pub remove: Vec<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum InitShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CompleteKind {
    /// workspace 名称
    Ws,
    /// project 名称
    Pj,
    /// tag key
    TagKey,
    /// tag value
    TagValue,
}

// ---------- tags ----------

#[derive(Subcommand, Debug)]
pub enum TagsCmd {
    /// 添加全局可用 tag
    #[command(visible_alias = "a")]
    Add {
        /// tag 列表，如 `OpenSrc License=MIT,BSD3.0`
        #[arg(required = true)]
        tags: Vec<String>,
    },

    /// 删除全局 tag（默认被引用时拒绝）
    #[command(visible_alias = "rm")]
    Remove {
        /// `key` 或 `key=value`
        #[arg(required = true)]
        tags: Vec<String>,

        /// 连同实体上的赋值一起删除
        #[arg(long)]
        force: bool,
    },

    /// 列出全局可用 tag
    #[command(visible_alias = "ls")]
    List {
        /// 仅列出指定 key
        #[arg(long, value_name = "KEY")]
        key: Option<String>,
    },
}

// ---------- workspace ----------

#[derive(Subcommand, Debug)]
pub enum WorkspaceCmd {
    /// 创建实体 workspace（无名称时创建单个 ws-<uuidv7> 并设为默认）
    #[command(visible_alias = "c")]
    Create {
        /// workspace 名称（可多个）；为空时创建单个默认 ws
        names: Vec<String>,

        /// 父目录（默认当前目录）
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,

        /// 创建为默认 workspace
        #[arg(long)]
        default: bool,

        #[command(flatten)]
        tag_args: TagArgs,
    },

    /// 将已存在的目录登记为 workspace
    #[command(visible_alias = "a")]
    Add {
        /// 目录路径
        path: PathBuf,

        /// workspace 名称（默认取目录名）
        #[arg(long, value_name = "NAME")]
        name: Option<String>,

        #[command(flatten)]
        tag_args: TagArgs,
    },

    /// 列出 workspace
    #[command(visible_alias = "ls")]
    List {
        /// 按 tag 过滤
        #[arg(long = "tag", value_name = "TAG", num_args = 1..)]
        tag: Vec<String>,

        /// 按选择器过滤
        #[arg(long = "select", value_name = "SELECTOR", num_args = 1..)]
        select: Vec<String>,
    },

    /// 更新 workspace 的 tag
    #[command(visible_alias = "up")]
    Update {
        /// workspace 名称
        name: String,

        #[command(flatten)]
        mutation: TagMutationArgs,
    },

    /// 重命名 workspace（同步 DefaultTags 首值）
    #[command(visible_alias = "rn")]
    Rename {
        /// 旧名称
        #[arg(value_name = "OLD")]
        old_name: String,
        /// 新名称
        #[arg(value_name = "NEW")]
        new_name: String,
    },

    /// 注销 workspace（默认保留磁盘目录）
    #[command(visible_alias = "rm")]
    Remove {
        /// workspace 名称
        name: String,

        /// 同时删除磁盘目录
        #[arg(long)]
        purge: bool,
    },

    /// 跳转到 workspace 目录
    #[command(visible_alias = "jp")]
    Jump {
        /// workspace 名称或选择器
        target: String,

        /// 仅打印目标路径（shell 集成内部使用）
        #[arg(long, hide = true)]
        print: bool,
    },

    /// 设置默认 workspace
    Use {
        /// workspace 名称
        name: String,
    },
}

// ---------- project ----------

#[derive(Subcommand, Debug)]
pub enum ProjectCmd {
    /// 在 workspace 内扁平创建 project 目录
    #[command(visible_alias = "c")]
    Create {
        /// project 名称（默认 pj-<uuidv7>）
        name: Option<String>,

        #[command(flatten)]
        select: SelectArgs,

        #[command(flatten)]
        tag_args: TagArgs,
    },

    /// 将已存在的任意外部目录登记为 project
    #[command(visible_alias = "a")]
    Add {
        /// 项目目录路径
        path: PathBuf,

        /// project 名称（默认取目录名）
        #[arg(long, value_name = "NAME")]
        name: Option<String>,

        #[command(flatten)]
        select: SelectArgs,

        #[command(flatten)]
        tag_args: TagArgs,
    },

    /// 列出 project
    #[command(visible_alias = "ls")]
    List {
        /// 列出所有 workspace 的 project
        #[arg(long)]
        all: bool,

        #[command(flatten)]
        select: SelectArgs,

        /// 按 tag 过滤
        #[arg(long = "tag", value_name = "TAG", num_args = 1..)]
        tag: Vec<String>,
    },

    /// 更新 project 的 tag
    #[command(visible_alias = "up")]
    Update {
        /// project 名称
        name: String,

        /// 同名 project 跨 workspace 时用于限定范围
        #[arg(long, value_name = "WS")]
        workspace: Option<String>,

        #[command(flatten)]
        mutation: TagMutationArgs,
    },

    /// 重命名 project（同步 DefaultTags 首值）
    #[command(visible_alias = "rn")]
    Rename {
        #[arg(value_name = "NAME")]
        name: String,

        #[arg(value_name = "NEW")]
        new_name: String,

        #[arg(long, value_name = "WS")]
        workspace: Option<String>,
    },

    /// 注销 project（默认保留磁盘目录）
    #[command(visible_alias = "rm")]
    Remove {
        /// project 名称
        name: String,

        /// 限定 workspace
        #[arg(long, value_name = "WS")]
        workspace: Option<String>,

        /// 同时删除磁盘目录
        #[arg(long)]
        purge: bool,
    },

    /// 将 project 移动到另一个 workspace（默认连磁盘目录一起移动）
    #[command(visible_alias = "mv")]
    Move {
        /// project 名称
        name: String,

        /// 目标 workspace
        #[arg(long, value_name = "WS")]
        to: String,

        /// 仅修改归属关系，不移动磁盘目录
        #[arg(long)]
        db_only: bool,
    },

    /// 跳转到 project 目录
    #[command(visible_alias = "jp")]
    Jump {
        /// project 名称
        name: String,

        #[command(flatten)]
        select: SelectArgs,

        /// 仅打印目标路径（shell 集成内部使用）
        #[arg(long, hide = true)]
        print: bool,
    },
}
