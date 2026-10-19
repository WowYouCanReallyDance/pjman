pub mod common;
pub mod complete;
pub mod completion;
pub mod init;
pub mod project;
pub mod tags_cmd;
pub mod workspace;

use crate::app::AppCtx;
use crate::cli::{Cli, Command};
use crate::error::{AppError, AppResult};

pub async fn run(cli: Cli, ctx: AppCtx) -> AppResult<()> {
    match cli.command {
        Command::Tags(cmd) => tags_cmd::run(cmd, &ctx).await,
        Command::Workspace(cmd) => workspace::run(cmd, &ctx).await,
        Command::Project(cmd) => project::run(cmd, &ctx).await,
        Command::Init { shell } => init::run(shell),
        Command::Completion { shell } => {
            completion::run(shell);
            Ok(())
        }
        Command::Complete { kind, partial } => complete::run(&ctx, kind, partial).await,
    }
}

// 保留：未接入命令的统一占位（如未来新增子命令）
#[allow(dead_code)]
pub(crate) fn todo(name: &str) -> AppResult<()> {
    Err(AppError::Unimplemented(name.into()))
}
