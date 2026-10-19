use crate::app::AppCtx;
use crate::cli::{Cli, Command};
use crate::error::{AppError, AppResult};

pub(crate) mod workspace;

pub async fn run(cli: Cli, ctx: AppCtx) -> AppResult<()> {
    match cli.command {
        Command::Tags(_) => Err(AppError::Unimplemented("tags".into())),
        Command::Workspace(cmd) => workspace::run(cmd, &ctx).await,
        Command::Project(_) => Err(AppError::Unimplemented("project".into())),
        Command::Init { .. } => Err(AppError::Unimplemented("init".into())),
        Command::Completion { .. } => Err(AppError::Unimplemented("completion".into())),
        // 动态补全在阶段 1 接入
        Command::Complete { .. } => Ok(()),
    }
}
