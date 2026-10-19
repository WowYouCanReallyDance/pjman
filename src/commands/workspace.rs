use std::io::IsTerminal;

use serde::Serialize;

use crate::app::AppCtx;
use crate::cli::WorkspaceCmd;
use crate::commands::common;
use crate::domain::tags::tokens_to_set;
use crate::error::{AppError, AppResult};
use crate::{output, service};

pub async fn run(cmd: WorkspaceCmd, ctx: &AppCtx) -> AppResult<()> {
    match cmd {
        WorkspaceCmd::Create {
            names,
            path,
            default,
            tag_args,
        } => create(ctx, names, path, default, tag_args.tags).await,
        WorkspaceCmd::Add {
            path,
            name,
            tag_args,
        } => add(ctx, path, name, tag_args.tags).await,
        WorkspaceCmd::List { tag, select } => list(ctx, tag, select).await,
        WorkspaceCmd::Update { name, mutation } => update(ctx, name, mutation).await,
        WorkspaceCmd::Rename { old_name, new_name } => rename(ctx, old_name, new_name).await,
        WorkspaceCmd::Remove { name, purge } => remove(ctx, name, purge).await,
        WorkspaceCmd::Jump { target, print } => jump(ctx, target, print).await,
        WorkspaceCmd::Use { name } => use_default(ctx, name).await,
    }
}

#[derive(Serialize)]
struct TagView {
    key: String,
    value: String,
    is_name: bool,
}

#[derive(Serialize)]
struct WsView {
    id: String,
    name: String,
    root_path: Option<String>,
    is_builtin: bool,
    missing: bool,
    tags: Vec<TagView>,
}

impl From<&service::workspace::WsDetail> for WsView {
    fn from(d: &service::workspace::WsDetail) -> Self {
        Self {
            id: d.model.id.clone(),
            name: d.model.name.clone(),
            root_path: d.model.root_path.clone(),
            is_builtin: d.model.is_builtin,
            missing: d.missing,
            tags: d
                .tags
                .iter()
                .map(|t| TagView {
                    key: t.key.clone(),
                    value: t.value.clone(),
                    is_name: t.is_name,
                })
                .collect(),
        }
    }
}

async fn create(
    ctx: &AppCtx,
    names: Vec<String>,
    parent: Option<std::path::PathBuf>,
    make_default: bool,
    tag_tokens: Vec<String>,
) -> AppResult<()> {
    let created =
        service::workspace::create(&ctx.db, names, parent, tag_tokens, make_default).await?;
    finish_views(ctx, created.iter().map(|m| m.name.clone()).collect()).await
}

async fn add(
    ctx: &AppCtx,
    path: std::path::PathBuf,
    name: Option<String>,
    tag_tokens: Vec<String>,
) -> AppResult<()> {
    let model = service::workspace::add_existing(&ctx.db, path, name, tag_tokens).await?;
    finish_views(ctx, vec![model.name]).await
}

/// 创建/添加后回显
async fn finish_views(ctx: &AppCtx, names: Vec<String>) -> AppResult<()> {
    let mut details = Vec::new();
    for name in names {
        let ws = service::workspace::by_name(&ctx.db, &name)
            .await?
            .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")))?;
        let tags = service::tags::entity_tags(&ctx.db, &service::tags::Target::Ws(&ws.id)).await?;
        let missing = ws
            .root_path
            .as_ref()
            .is_some_and(|p| !std::path::Path::new(p).exists());
        details.push(service::workspace::WsDetail {
            model: ws,
            tags,
            missing,
        });
    }

    if ctx.json {
        let views: Vec<WsView> = details.iter().map(WsView::from).collect();
        return output::print_json(&views);
    }
    for d in &details {
        match &d.model.root_path {
            Some(path) => println!("workspace `{}` → {}", d.model.name, path),
            None => println!("workspace `{}` (virtual)", d.model.name),
        }
    }
    Ok(())
}

async fn list(ctx: &AppCtx, tag: Vec<String>, select: Vec<String>) -> AppResult<()> {
    let filter_tokens = if !select.is_empty() { select } else { tag };
    let filter = if filter_tokens.is_empty() {
        None
    } else {
        Some(tokens_to_set(&filter_tokens)?)
    };
    let details = service::workspace::list(&ctx.db, filter.as_ref()).await?;

    if ctx.json {
        let views: Vec<WsView> = details.iter().map(WsView::from).collect();
        output::print_json(&views)?;
        return Ok(());
    }

    let rows = details
        .iter()
        .map(|d| {
            let tags = d
                .tags
                .iter()
                .filter(|t| !t.is_name)
                .map(|t| format!("{}={}", t.key, t.value))
                .collect::<Vec<_>>()
                .join(",");
            let path = match &d.model.root_path {
                Some(p) if d.missing => format!("{p} (missing)"),
                Some(p) => p.clone(),
                None => "(virtual)".to_string(),
            };
            vec![d.model.name.clone(), tags, path]
        })
        .collect();
    output::print_table(&["NAME", "TAGS", "PATH"], rows);
    Ok(())
}

async fn update(
    ctx: &AppCtx,
    name: String,
    mutation: crate::cli::TagMutationArgs,
) -> AppResult<()> {
    let mutation = common::mutation(mutation)?;
    if mutation.is_empty() {
        return Err(AppError::invalid_args("至少提供一个 --add/--set/--remove"));
    }
    service::workspace::update_tags(&ctx.db, &name, &mutation).await?;
    println!("已更新 workspace `{name}` 的 tag");
    Ok(())
}

async fn rename(ctx: &AppCtx, old: String, new: String) -> AppResult<()> {
    service::workspace::rename(&ctx.db, &old, &new).await?;
    println!("已重命名 workspace：{old} → {new}");
    Ok(())
}

async fn remove(ctx: &AppCtx, name: String, purge: bool) -> AppResult<()> {
    if purge && !ctx.yes {
        let ws = service::workspace::by_name(&ctx.db, &name).await?;
        if let Some(root) = ws.as_ref().and_then(|w| w.root_path.clone()) {
            let path = std::path::PathBuf::from(&root);
            if path.exists() && std::io::stdin().is_terminal() {
                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!("确认删除目录 {root}？此操作不可恢复"))
                    .default(false)
                    .interact_opt()
                    .map_err(|e| AppError::invalid_args(e.to_string()))?
                    .unwrap_or(false);
                if !confirmed {
                    return Err(AppError::invalid_args("已取消"));
                }
            }
        }
    }
    service::workspace::remove(&ctx.db, &name, purge).await?;
    println!(
        "已注销 workspace `{name}`{}",
        if purge {
            "（目录已删除）"
        } else {
            "（保留磁盘目录）"
        }
    );
    Ok(())
}

async fn jump(ctx: &AppCtx, target: String, print: bool) -> AppResult<()> {
    // target 先按名称解析；形如 k=v 时按单 token 选择器解析
    let path = if target.contains('=') {
        let ws = common::resolve_ws(
            ctx,
            &crate::cli::SelectArgs {
                workspace: None,
                select: vec![target],
            },
            service::workspace::Fallback::Error,
        )
        .await?;
        service::workspace::jump_path(&ctx.db, &ws.name).await?
    } else {
        service::workspace::jump_path(&ctx.db, &target).await?
    };
    if print {
        println!("{path}");
    } else {
        eprintln!("请通过 shell 集成跳转：eval \"$(pjman init zsh)\"，或手动 cd {path}");
    }
    Ok(())
}

async fn use_default(ctx: &AppCtx, name: String) -> AppResult<()> {
    service::workspace::set_default(&ctx.db, &name).await?;
    println!("默认 workspace 已设置为 `{name}`");
    Ok(())
}
