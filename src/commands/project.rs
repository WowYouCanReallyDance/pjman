use std::io::IsTerminal as _;

use serde::Serialize;

use crate::app::AppCtx;
use crate::cli::ProjectCmd;
use crate::commands::common;
use crate::domain::tags::tokens_to_set;
use crate::error::{AppError, AppResult};
use crate::{output, service};

pub async fn run(cmd: ProjectCmd, ctx: &AppCtx) -> AppResult<()> {
    match cmd {
        ProjectCmd::Create {
            name,
            select,
            tag_args,
        } => create(ctx, name, select, tag_args.tags).await,
        ProjectCmd::Add {
            path,
            name,
            select,
            tag_args,
        } => add(ctx, path, name, select, tag_args.tags).await,
        ProjectCmd::List { all, select, tag } => list(ctx, all, select, tag).await,
        ProjectCmd::Update {
            name,
            workspace,
            mutation,
        } => update(ctx, name, workspace, mutation).await,
        ProjectCmd::Rename {
            name,
            new_name,
            workspace,
        } => rename(ctx, name, new_name, workspace).await,
        ProjectCmd::Remove {
            name,
            workspace,
            purge,
        } => remove(ctx, name, workspace, purge).await,
        ProjectCmd::Move { name, to, db_only } => move_to(ctx, name, to, db_only).await,
        ProjectCmd::Jump {
            name,
            select,
            print,
        } => jump(ctx, name, select, print).await,
    }
}

#[derive(Serialize)]
struct PjView {
    id: String,
    name: String,
    workspace: String,
    path: String,
    missing: bool,
    tags: Vec<TagView>,
}

#[derive(Serialize)]
struct TagView {
    key: String,
    value: String,
    is_name: bool,
}

impl From<&service::project::PjDetail> for PjView {
    fn from(d: &service::project::PjDetail) -> Self {
        PjView {
            id: d.model.id.clone(),
            name: d.model.name.clone(),
            workspace: d.ws_name.clone(),
            path: d.model.path.clone(),
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

/// `--workspace <name>` 限定范围（精确名称，缺省 None）
async fn scope(
    ctx: &AppCtx,
    name: &Option<String>,
) -> AppResult<Option<crate::entity::workspace::Model>> {
    match name {
        Some(name) => Ok(Some(
            service::workspace::by_name(&ctx.db, name)
                .await?
                .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")))?,
        )),
        None => Ok(None),
    }
}

async fn create(
    ctx: &AppCtx,
    name: Option<String>,
    select: crate::cli::SelectArgs,
    tag_tokens: Vec<String>,
) -> AppResult<()> {
    let ws = common::resolve_ws(ctx, &select, service::workspace::Fallback::DefaultWs).await?;
    let pj = service::project::create(&ctx.db, name, &ws, tag_tokens).await?;
    finish(ctx, vec![pj.id], false).await
}

async fn add(
    ctx: &AppCtx,
    path: std::path::PathBuf,
    name: Option<String>,
    select: crate::cli::SelectArgs,
    tag_tokens: Vec<String>,
) -> AppResult<()> {
    let ws = common::resolve_ws(ctx, &select, service::workspace::Fallback::Dvws).await?;
    let pj = service::project::add_existing(&ctx.db, path, name, &ws, tag_tokens).await?;
    finish(ctx, vec![pj.id], false).await
}

async fn list(
    ctx: &AppCtx,
    _all: bool,
    select: crate::cli::SelectArgs,
    tag: Vec<String>,
) -> AppResult<()> {
    let ws_scope = scope(ctx, &select.workspace).await?;
    let filter_tokens = if !select.select.is_empty() {
        select.select
    } else {
        tag
    };
    let filter = if filter_tokens.is_empty() {
        None
    } else {
        Some(tokens_to_set(&filter_tokens)?)
    };
    let details = service::project::list(&ctx.db, ws_scope.as_ref(), filter.as_ref()).await?;

    if ctx.json {
        let views: Vec<PjView> = details.iter().map(PjView::from).collect();
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
            let path = if d.missing {
                format!("{} (missing)", d.model.path)
            } else {
                d.model.path.clone()
            };
            vec![d.model.name.clone(), d.ws_name.clone(), tags, path]
        })
        .collect();
    output::print_table(&["NAME", "WORKSPACE", "TAGS", "PATH"], rows);
    Ok(())
}

async fn update(
    ctx: &AppCtx,
    name: String,
    workspace: Option<String>,
    mutation: crate::cli::TagMutationArgs,
) -> AppResult<()> {
    let ws_scope = scope(ctx, &workspace).await?;
    let mutation = common::mutation(mutation)?;
    if mutation.is_empty() {
        return Err(AppError::invalid_args("至少提供一个 --add/--set/--remove"));
    }
    service::project::update_tags(&ctx.db, &name, ws_scope.as_ref(), &mutation).await?;
    println!("已更新 project `{name}` 的 tag");
    Ok(())
}

async fn rename(
    ctx: &AppCtx,
    name: String,
    new_name: String,
    workspace: Option<String>,
) -> AppResult<()> {
    let ws_scope = scope(ctx, &workspace).await?;
    service::project::rename(&ctx.db, &name, &new_name, ws_scope.as_ref()).await?;
    println!("已重命名 project：{name} → {new_name}");
    Ok(())
}

async fn remove(
    ctx: &AppCtx,
    name: String,
    workspace: Option<String>,
    purge: bool,
) -> AppResult<()> {
    if purge && !ctx.yes && std::io::stdin().is_terminal() {
        let ws_scope = scope(ctx, &workspace).await?;
        if let Ok(pj) = service::project::require_named(&ctx.db, &name, ws_scope.as_ref()).await {
            if std::path::Path::new(&pj.path).exists() {
                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!("确认删除目录 {}？此操作不可恢复", pj.path))
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
    let ws_scope = scope(ctx, &workspace).await?;
    service::project::remove(&ctx.db, &name, ws_scope.as_ref(), purge).await?;
    println!(
        "已注销 project `{name}`{}",
        if purge {
            "（目录已删除）"
        } else {
            "（保留磁盘目录）"
        }
    );
    Ok(())
}

async fn move_to(ctx: &AppCtx, name: String, to: String, db_only: bool) -> AppResult<()> {
    // 源 ws 不做限定，同名歧义时再要求 --workspace
    let target = service::workspace::by_name(&ctx.db, &to)
        .await?
        .ok_or_else(|| AppError::not_found(format!("目标 workspace 不存在：{to}")))?;
    let pj = service::project::move_to(&ctx.db, &name, None, &target, db_only).await?;
    println!(
        "project `{}` 已移动到 workspace `{}`{}",
        pj.name,
        to,
        if db_only || target.root_path.is_none() {
            "（仅归属关系）"
        } else {
            "（含磁盘目录）"
        }
    );
    Ok(())
}

async fn jump(
    ctx: &AppCtx,
    name: String,
    select: crate::cli::SelectArgs,
    print: bool,
) -> AppResult<()> {
    let ws_scope = scope(ctx, &select.workspace).await?;
    let path = service::project::jump_path(&ctx.db, &name, ws_scope.as_ref()).await?;
    if print {
        println!("{path}");
    } else {
        eprintln!("请通过 shell 集成跳转：eval \"$(pjman init zsh)\"，或手动 cd {path}");
    }
    Ok(())
}

/// 创建/添加后回显
async fn finish(ctx: &AppCtx, ids: Vec<String>, _json_only: bool) -> AppResult<()> {
    let details = service::project::list_by_ids(&ctx.db, &ids).await?;
    if ctx.json {
        let views: Vec<PjView> = details.iter().map(PjView::from).collect();
        output::print_json(&views)?;
        return Ok(());
    }
    for d in &details {
        println!(
            "project `{}` → {} (workspace: {})",
            d.model.name, d.model.path, d.ws_name
        );
    }
    Ok(())
}
