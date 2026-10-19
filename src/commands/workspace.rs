use serde::Serialize;

use crate::app::AppCtx;
use crate::cli::WorkspaceCmd;
use crate::entity::workspace;
use crate::error::{AppError, AppResult};
use crate::{output, service};

pub async fn run(cmd: WorkspaceCmd, ctx: &AppCtx) -> AppResult<()> {
    match cmd {
        WorkspaceCmd::List { .. } => list(ctx).await,
        WorkspaceCmd::Create { .. } => Err(AppError::Unimplemented("ws create".into())),
        WorkspaceCmd::Add { .. } => Err(AppError::Unimplemented("ws add".into())),
        WorkspaceCmd::Update { .. } => Err(AppError::Unimplemented("ws update".into())),
        WorkspaceCmd::Rename { .. } => Err(AppError::Unimplemented("ws rename".into())),
        WorkspaceCmd::Remove { .. } => Err(AppError::Unimplemented("ws remove".into())),
        WorkspaceCmd::Jump { .. } => Err(AppError::Unimplemented("ws jump".into())),
        WorkspaceCmd::Use { .. } => Err(AppError::Unimplemented("ws use".into())),
    }
}

#[derive(Serialize)]
struct WsView {
    id: String,
    name: String,
    root_path: Option<String>,
    is_builtin: bool,
}

impl From<&workspace::Model> for WsView {
    fn from(model: &workspace::Model) -> Self {
        Self {
            id: model.id.clone(),
            name: model.name.clone(),
            root_path: model.root_path.clone(),
            is_builtin: model.is_builtin,
        }
    }
}

async fn list(ctx: &AppCtx) -> AppResult<()> {
    let models = service::workspace::list(&ctx.db).await?;

    if ctx.json {
        let views: Vec<WsView> = models.iter().map(WsView::from).collect();
        return output::print_json(&views);
    }

    let rows = models
        .iter()
        .map(|m| {
            vec![
                m.name.clone(),
                m.root_path
                    .clone()
                    .unwrap_or_else(|| "(virtual)".to_string()),
            ]
        })
        .collect();
    output::print_table(&["NAME", "PATH"], rows);
    Ok(())
}
