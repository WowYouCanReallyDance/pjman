use std::path::{Path, PathBuf};

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, TransactionTrait,
};

use crate::domain::{ids, paths, tags::TagSet, tags::tokens_to_set};
use crate::entity::{project, project_tag, tag, workspace};
use crate::error::{AppError, AppResult};
use crate::service::tags::{self, EntityTag, TagMutation, Target, TargetKind};

#[derive(Debug, Clone)]
pub struct PjDetail {
    pub model: project::Model,
    pub ws_name: String,
    pub tags: Vec<EntityTag>,
    pub missing: bool,
}

// ---------- 查询 ----------

async fn by_id<C: ConnectionTrait>(db: &C, id: &str) -> AppResult<project::Model> {
    project::Entity::find()
        .filter(project::Column::Id.eq(id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::not_found(format!("project 不存在：{id}")))
}

/// 按名称（可选 workspace 范围）查找；跨 ws 可能有多个
async fn find_named(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
) -> AppResult<Vec<project::Model>> {
    let mut query = project::Entity::find().filter(project::Column::Name.eq(name));
    if let Some(ws) = ws_scope {
        query = query.filter(project::Column::WorkspaceId.eq(ws.id.clone()));
    }
    Ok(query.all(db).await?)
}

/// 解析唯一个 project，歧义时要求 --workspace
pub async fn require_named(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
) -> AppResult<project::Model> {
    let mut rows = find_named(db, name, ws_scope).await?;
    match rows.len() {
        0 => Err(AppError::not_found(match ws_scope {
            Some(ws) => format!("workspace `{}` 下不存在 project：{name}", ws.name),
            None => format!("project 不存在：{name}"),
        })),
        1 => Ok(rows.remove(0)),
        _ => {
            // 补上各自 ws 名称作为候选
            let mut candidates = Vec::new();
            for row in &rows {
                if let Some(ws) = workspace::Entity::find()
                    .filter(workspace::Column::Id.eq(&row.workspace_id))
                    .one(db)
                    .await?
                {
                    candidates.push(format!("--workspace {}", ws.name));
                }
            }
            Err(AppError::ambiguous(
                format!("project `{name}` 存在于多个 workspace，请用 --workspace 限定："),
                candidates,
            ))
        }
    }
}

/// 列出 project，可限定 ws、按 tag 过滤
pub async fn list(
    db: &DatabaseConnection,
    ws_scope: Option<&workspace::Model>,
    filter: Option<&TagSet>,
) -> AppResult<Vec<PjDetail>> {
    let mut models = match ws_scope {
        Some(ws) => {
            project::Entity::find()
                .filter(project::Column::WorkspaceId.eq(ws.id.clone()))
                .all(db)
                .await?
        }
        None => project::Entity::find().all(db).await?,
    };

    if let Some(set) = filter {
        if !set.is_empty() {
            let ids = tags::matching_ids(db, TargetKind::Pj, set).await?;
            models.retain(|m| ids.contains(&m.id));
        }
    }

    details_from_models(db, models).await
}

/// 按 id 列表取详情（创建后回显用）
pub async fn list_by_ids(db: &DatabaseConnection, ids: &[String]) -> AppResult<Vec<PjDetail>> {
    let models = project::Entity::find()
        .filter(project::Column::Id.is_in(ids.to_owned()))
        .all(db)
        .await?;
    details_from_models(db, models).await
}

async fn details_from_models(
    db: &DatabaseConnection,
    models: Vec<project::Model>,
) -> AppResult<Vec<PjDetail>> {
    let pj_ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let mut tags_map = tags::tags_of_entities(db, TargetKind::Pj, &pj_ids).await?;
    let ws_rows = workspace::Entity::find().all(db).await?;
    let ws_names: std::collections::HashMap<String, String> =
        ws_rows.into_iter().map(|w| (w.id, w.name)).collect();

    Ok(models
        .into_iter()
        .map(|m| {
            let missing = !Path::new(&m.path).exists();
            let ws_name = ws_names.get(&m.workspace_id).cloned().unwrap_or_default();
            let pj_tags = tags_map.remove(&m.id).unwrap_or_default();
            PjDetail {
                model: m,
                ws_name,
                tags: pj_tags,
                missing,
            }
        })
        .collect())
}

// ---------- 创建 / 添加 ----------

pub async fn create(
    db: &DatabaseConnection,
    name: Option<String>,
    ws: &workspace::Model,
    tag_tokens: Vec<String>,
) -> AppResult<project::Model> {
    let root = ws.root_path.clone().ok_or_else(|| {
        AppError::invalid_args(format!(
            "workspace `{}` 是虚拟 workspace，无法在其中创建目录；请用 `pj add` 登记已有项目",
            ws.name
        ))
    })?;
    let name = name.unwrap_or_else(|| format!("pj-{}", ids::new_id()));
    let dir = PathBuf::from(&root).join(&name);
    if dir.exists() {
        return Err(AppError::conflict(format!("目录已存在：{}", dir.display())));
    }

    validate_new_project(db, &ws.id, &name, &dir).await?;

    let user_tags = tokens_to_set(&tag_tokens)?;
    std::fs::create_dir(&dir)?;
    let abs_path = paths::resolve(&dir)?;
    let pj_id = ids::new_id();

    let model = db
        .transaction(|txn| {
            let pj_id = pj_id.clone();
            let name = name.clone();
            let abs_path = abs_path.clone();
            let ws_id = ws.id.clone();
            let user_tags = user_tags.clone();
            Box::pin(async move {
                let now = chrono::Utc::now();
                project::Entity::insert(project::ActiveModel {
                    id: Set(pj_id.clone()),
                    workspace_id: Set(ws_id),
                    name: Set(name.clone()),
                    path: Set(abs_path.to_string_lossy().to_string()),
                    created_at: Set(now),
                    updated_at: Set(now),
                })
                .exec(txn)
                .await?;
                attach_name_and_tags(txn, &pj_id, &name, &user_tags).await?;
                by_id(txn, &pj_id).await
            })
        })
        .await?;
    Ok(model)
}

pub async fn add_existing(
    db: &DatabaseConnection,
    path: PathBuf,
    name: Option<String>,
    ws: &workspace::Model,
    tag_tokens: Vec<String>,
) -> AppResult<project::Model> {
    let abs_path = paths::resolve(&path)?;
    if !abs_path.is_dir() {
        return Err(AppError::invalid_args(format!(
            "路径不存在或不是目录：{}",
            abs_path.display()
        )));
    }
    let name = match name {
        Some(n) => n,
        None => paths::file_name(&abs_path)?,
    };
    validate_new_project(db, &ws.id, &name, &abs_path).await?;

    let user_tags = tokens_to_set(&tag_tokens)?;
    let pj_id = ids::new_id();
    let model = db
        .transaction(|txn| {
            let pj_id = pj_id.clone();
            let name = name.clone();
            let abs_path = abs_path.clone();
            let ws_id = ws.id.clone();
            let user_tags = user_tags.clone();
            Box::pin(async move {
                let now = chrono::Utc::now();
                project::Entity::insert(project::ActiveModel {
                    id: Set(pj_id.clone()),
                    workspace_id: Set(ws_id),
                    name: Set(name.clone()),
                    path: Set(abs_path.to_string_lossy().to_string()),
                    created_at: Set(now),
                    updated_at: Set(now),
                })
                .exec(txn)
                .await?;
                attach_name_and_tags(txn, &pj_id, &name, &user_tags).await?;
                by_id(txn, &pj_id).await
            })
        })
        .await?;
    Ok(model)
}

async fn validate_new_project(
    db: &DatabaseConnection,
    ws_id: &str,
    name: &str,
    path: &Path,
) -> AppResult<()> {
    let same_name = project::Entity::find()
        .filter(project::Column::WorkspaceId.eq(ws_id))
        .filter(project::Column::Name.eq(name))
        .count(db)
        .await?;
    if same_name > 0 {
        return Err(AppError::conflict(format!(
            "workspace 下已存在同名 project：{name}"
        )));
    }
    let path_str = path.to_string_lossy().to_string();
    let same_path = project::Entity::find()
        .filter(project::Column::Path.eq(path_str))
        .count(db)
        .await?;
    if same_path > 0 {
        return Err(AppError::conflict(format!(
            "目录已被登记为 project：{}",
            path.display()
        )));
    }
    Ok(())
}

async fn attach_name_and_tags<C: ConnectionTrait>(
    txn: &C,
    pj_id: &str,
    name: &str,
    user_tags: &TagSet,
) -> AppResult<()> {
    let name_tag_id = tags::ensure_tag(txn, crate::domain::tags::DEFAULT_TAGS_KEY, name).await?;
    let name_tag = tag::Model {
        id: name_tag_id,
        tag_key: crate::domain::tags::DEFAULT_TAGS_KEY.to_owned(),
        tag_value: name.to_owned(),
        created_at: chrono::Utc::now(),
    };
    tags::attach(txn, &Target::Pj(pj_id), &[name_tag], true).await?;
    if !user_tags.is_empty() {
        let extra = tags::ensure_tag_set(txn, user_tags).await?;
        tags::attach(txn, &Target::Pj(pj_id), &extra, false).await?;
    }
    Ok(())
}

// ---------- 变更 ----------

pub async fn update_tags(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
    mutation: &TagMutation,
) -> AppResult<()> {
    let pj = require_named(db, name, ws_scope).await?;
    db.transaction(|txn| {
        let pj_id = pj.id.clone();
        let mutation = mutation.clone();
        Box::pin(async move { tags::apply_mutation(txn, &Target::Pj(&pj_id), &mutation).await })
    })
    .await?;
    Ok(())
}

pub async fn rename(
    db: &DatabaseConnection,
    name: &str,
    new_name: &str,
    ws_scope: Option<&workspace::Model>,
) -> AppResult<()> {
    let pj = require_named(db, name, ws_scope).await?;
    let dup = project::Entity::find()
        .filter(project::Column::WorkspaceId.eq(&pj.workspace_id))
        .filter(project::Column::Name.eq(new_name))
        .count(db)
        .await?;
    if dup > 0 {
        return Err(AppError::conflict(format!(
            "workspace 下已存在同名 project：{new_name}"
        )));
    }

    db.transaction(|txn| {
        let pj_id = pj.id.clone();
        let new_name = new_name.to_owned();
        Box::pin(async move {
            let now = chrono::Utc::now();
            let mut am: project::ActiveModel = by_id(txn, &pj_id).await?.into();
            am.name = Set(new_name.clone());
            am.updated_at = Set(now);
            am.update(txn).await?;

            let current = tags::entity_tags(txn, &Target::Pj(&pj_id)).await?;
            if let Some(name_tag) = current.into_iter().find(|t| t.is_name) {
                let mut tag_am: tag::ActiveModel = tag::Entity::find()
                    .filter(tag::Column::Id.eq(name_tag.id))
                    .one(txn)
                    .await?
                    .unwrap()
                    .into();
                tag_am.tag_value = Set(new_name);
                tag_am.update(txn).await?;
            }
            Ok(())
        })
    })
    .await?;
    Ok(())
}

pub async fn remove(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
    purge: bool,
) -> AppResult<()> {
    let pj = require_named(db, name, ws_scope).await?;
    if purge && Path::new(&pj.path).exists() {
        let path = PathBuf::from(&pj.path);
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(path)).await??;
    }
    project_tag::Entity::delete_many()
        .filter(project_tag::Column::ProjectId.eq(&pj.id))
        .exec(db)
        .await?;
    project::Entity::delete_many()
        .filter(project::Column::Id.eq(&pj.id))
        .exec(db)
        .await?;
    Ok(())
}

/// 移动 project 到目标 ws；默认连磁盘目录一起移动
pub async fn move_to(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
    target_ws: &workspace::Model,
    db_only: bool,
) -> AppResult<project::Model> {
    let pj = require_named(db, name, ws_scope).await?;
    if pj.workspace_id == target_ws.id {
        return Err(AppError::invalid_args(format!(
            "project 已在 workspace `{}` 中",
            target_ws.name
        )));
    }
    // 目标 ws 同名检查
    let dup = project::Entity::find()
        .filter(project::Column::WorkspaceId.eq(&target_ws.id))
        .filter(project::Column::Name.eq(&pj.name))
        .count(db)
        .await?;
    if dup > 0 {
        return Err(AppError::conflict(format!(
            "目标 workspace `{}` 已存在同名 project `{}`",
            target_ws.name, pj.name
        )));
    }

    let mut new_path = pj.path.clone();
    if !db_only {
        // 目标是实体 ws：连磁盘目录一起移动；虚拟 ws 自动退化为仅改归属
        if let Some(target_root) = &target_ws.root_path {
            let dst = PathBuf::from(target_root).join(&pj.name);
            if dst.exists() {
                return Err(AppError::conflict(format!(
                    "目标目录已存在：{}",
                    dst.display()
                )));
            }
            let src = PathBuf::from(&pj.path);
            if src.exists() {
                if let Err(e) = std::fs::rename(&src, &dst) {
                    if e.raw_os_error() == Some(libc_cross_device()) {
                        copy_dir_all(&src, &dst).await?;
                        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(src)).await??;
                    } else {
                        return Err(e.into());
                    }
                }
            }
            new_path = paths::resolve(&dst)?.to_string_lossy().to_string();
        }
    }

    let now = chrono::Utc::now();
    let mut am: project::ActiveModel = pj.clone().into();
    am.workspace_id = Set(target_ws.id.clone());
    am.path = Set(new_path);
    am.updated_at = Set(now);
    let updated = am.update(db).await?;
    Ok(updated)
}

pub async fn jump_path(
    db: &DatabaseConnection,
    name: &str,
    ws_scope: Option<&workspace::Model>,
) -> AppResult<String> {
    let pj = require_named(db, name, ws_scope).await?;
    if Path::new(&pj.path).is_dir() {
        Ok(pj.path)
    } else {
        Err(AppError::not_found(format!(
            "project `{}` 的目录已不存在（断链）：{}",
            pj.name, pj.path
        )))
    }
}

// ---------- 辅助 ----------

/// EXDEV（跨文件设备）错误码
#[cfg(unix)]
fn libc_cross_device() -> i32 {
    18
}

#[cfg(not(unix))]
fn libc_cross_device() -> i32 {
    -1
}

async fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();
    tokio::task::spawn_blocking(move || copy_recursive(&src, &dst)).await??;
    Ok(())
}

fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
