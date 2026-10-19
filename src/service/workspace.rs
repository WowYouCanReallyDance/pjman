use std::path::PathBuf;

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, TransactionTrait,
};

use crate::db::meta;
use crate::domain::{
    ids, paths,
    tags::{TagSet, tokens_to_set},
};
use crate::entity::{project, tag, workspace, workspace_tag};
use crate::error::{AppError, AppResult};
use crate::service::tags::{self, EntityTag, TagMutation, Target, TargetKind};

/// 选择 workspace 的三种输入
pub struct WsSelector {
    pub name: Option<String>,
    pub select: Vec<String>,
}

/// 未显式选择时的回退策略
pub enum Fallback {
    /// 使用 meta.default_ws_id
    DefaultWs,
    /// 使用内置虚拟 ws dvws
    Dvws,
    /// 不允许回退，必须显式选中
    Error,
}

#[derive(Debug, Clone)]
pub struct WsDetail {
    pub model: workspace::Model,
    pub tags: Vec<EntityTag>,
    pub missing: bool,
}

// ---------- 查询 ----------

pub async fn by_name(db: &DatabaseConnection, name: &str) -> AppResult<Option<workspace::Model>> {
    Ok(workspace::Entity::find()
        .filter(workspace::Column::Name.eq(name))
        .one(db)
        .await?)
}

async fn by_id<C: ConnectionTrait>(db: &C, id: &str) -> AppResult<workspace::Model> {
    workspace::Entity::find()
        .filter(workspace::Column::Id.eq(id))
        .one(db)
        .await?
        .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{id}")))
}

/// 带 tag 与断链标记的列表，可按 TagSet 过滤
pub async fn list(db: &DatabaseConnection, filter: Option<&TagSet>) -> AppResult<Vec<WsDetail>> {
    let models = workspace::Entity::find()
        .order_by_desc(workspace::Column::IsBuiltin)
        .order_by_asc(workspace::Column::Name)
        .all(db)
        .await?;

    let models = match filter {
        Some(set) if !set.is_empty() => {
            let ids = tags::matching_ids(db, TargetKind::Ws, set).await?;
            models.into_iter().filter(|m| ids.contains(&m.id)).collect()
        }
        _ => models,
    };

    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let mut tags_map = tags::tags_of_entities(db, TargetKind::Ws, &ids).await?;

    Ok(models
        .into_iter()
        .map(|m| {
            let missing = m
                .root_path
                .as_ref()
                .is_some_and(|p| !std::path::Path::new(p).exists());
            WsDetail {
                tags: tags_map.remove(&m.id).unwrap_or_default(),
                model: m,
                missing,
            }
        })
        .collect())
}

/// 解析目标 workspace：--workspace > --select > 回退策略
pub async fn resolve(
    db: &DatabaseConnection,
    selector: WsSelector,
    fallback: Fallback,
) -> AppResult<workspace::Model> {
    if let Some(name) = selector.name {
        return by_name(db, &name)
            .await?
            .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")));
    }

    if !selector.select.is_empty() {
        let required = tokens_to_set(&selector.select)?;
        let ids = tags::matching_ids(db, TargetKind::Ws, &required).await?;
        let mut matched = Vec::new();
        for id in ids {
            matched.push(by_id(db, &id).await?);
        }
        return match matched.len() {
            0 => Err(AppError::not_found(format!(
                "没有 workspace 满足选择器：{}",
                selector.select.join(" ")
            ))),
            1 => Ok(matched.pop().unwrap()),
            _ => {
                let candidates = matched
                    .iter()
                    .map(|m| format!("{}={}", crate::domain::tags::DEFAULT_TAGS_KEY, m.name))
                    .collect();
                Err(AppError::ambiguous(
                    "选择器命中多个 workspace，请追加 DefaultTags=<名称> 收敛：",
                    candidates,
                ))
            }
        };
    }

    match fallback {
        Fallback::DefaultWs => match meta::get(db, meta::DEFAULT_WS_KEY).await? {
            Some(id) => by_id(db, &id).await,
            None => Err(AppError::not_found(
                "尚未设置默认 workspace；请先用 `pjman ws create` 创建或 `pjman ws use <name>` 指定",
            )),
        },
        Fallback::Dvws => by_id(db, &ids::DVWS_ID).await,
        Fallback::Error => Err(AppError::invalid_args(
            "请用 --workspace <名称> 或 --select <tag...> 指定 workspace",
        )),
    }
}

// ---------- 创建 / 添加 ----------

/// 创建一个或多个实体 workspace 目录并登记
pub async fn create(
    db: &DatabaseConnection,
    names: Vec<String>,
    parent: Option<PathBuf>,
    tag_tokens: Vec<String>,
    make_default: bool,
) -> AppResult<Vec<workspace::Model>> {
    let user_tags = tokens_to_set(&tag_tokens)?;

    // 无名称：创建单个 ws-<uuidv7> 并作为默认
    let (names, bare) = if names.is_empty() {
        (vec![format!("ws-{}", ids::new_id())], true)
    } else {
        (names, false)
    };

    let parent = paths::resolve(&parent.unwrap_or_else(|| std::path::PathBuf::from(".")))?;
    if !parent.is_dir() {
        return Err(AppError::invalid_args(format!(
            "父目录不存在或不是目录：{}",
            parent.display()
        )));
    }

    // 先做全部冲突校验，避免留下半成品目录
    for name in &names {
        if by_name(db, name).await?.is_some() {
            return Err(AppError::conflict(format!("workspace 名称已存在：{name}")));
        }
        if parent.join(name).exists() {
            return Err(AppError::conflict(format!(
                "目录已存在：{}",
                parent.join(name).display()
            )));
        }
    }

    let mut created = Vec::new();
    for name in &names {
        let dir = parent.join(name);
        std::fs::create_dir(&dir)?;
        let root_path = paths::resolve(&dir)?;
        let ws_id = ids::new_id();

        let model = db
            .transaction(|txn| {
                let name = name.clone();
                let root_path = root_path.clone();
                let user_tags = user_tags.clone();
                let ws_id = ws_id.clone();
                Box::pin(async move {
                    let now = chrono::Utc::now();
                    let ws = workspace::ActiveModel {
                        id: Set(ws_id.clone()),
                        name: Set(name.clone()),
                        root_path: Set(Some(root_path.to_string_lossy().to_string())),
                        is_builtin: Set(false),
                        created_at: Set(now),
                        updated_at: Set(now),
                    };
                    workspace::Entity::insert(ws).exec(txn).await?;

                    let name_tag = ensure_name_tag(txn, &name).await?;
                    tags::attach(txn, &Target::Ws(&ws_id), &[name_tag], true).await?;
                    if !user_tags.is_empty() {
                        let extra = tags::ensure_tag_set(txn, &user_tags).await?;
                        tags::attach(txn, &Target::Ws(&ws_id), &extra, false).await?;
                    }
                    by_id(txn, &ws_id).await
                })
            })
            .await?;
        created.push(model);
    }

    // 默认 ws：裸创建必然设置；显式创建仅在 --default 或尚无默认时设置首个
    let should_set_default =
        bare || make_default || meta::get(db, meta::DEFAULT_WS_KEY).await?.is_none();
    if should_set_default {
        if let Some(first) = created.first() {
            meta::set(db, meta::DEFAULT_WS_KEY, &first.id).await?;
        }
    }

    Ok(created)
}

/// 将已存在目录登记为实体 workspace
pub async fn add_existing(
    db: &DatabaseConnection,
    path: PathBuf,
    name: Option<String>,
    tag_tokens: Vec<String>,
) -> AppResult<workspace::Model> {
    let root_path = paths::resolve(&path)?;
    if !root_path.is_dir() {
        return Err(AppError::invalid_args(format!(
            "路径不存在或不是目录：{}",
            root_path.display()
        )));
    }
    let name = name.unwrap_or_else(|| paths::file_name(&root_path).unwrap_or_default());
    if name.is_empty() {
        return Err(AppError::invalid_args(
            "无法从路径推断 workspace 名称，请用 --name 指定",
        ));
    }
    if by_name(db, &name).await?.is_some() {
        return Err(AppError::conflict(format!("workspace 名称已存在：{name}")));
    }
    let user_tags = tokens_to_set(&tag_tokens)?;

    let ws_id = ids::new_id();
    let model = db
        .transaction(|txn| {
            let name = name.clone();
            let root_path = root_path.clone();
            let user_tags = user_tags.clone();
            let ws_id = ws_id.clone();
            Box::pin(async move {
                let now = chrono::Utc::now();
                let ws = workspace::ActiveModel {
                    id: Set(ws_id.clone()),
                    name: Set(name.clone()),
                    root_path: Set(Some(root_path.to_string_lossy().to_string())),
                    is_builtin: Set(false),
                    created_at: Set(now),
                    updated_at: Set(now),
                };
                workspace::Entity::insert(ws).exec(txn).await?;
                let name_tag = ensure_name_tag(txn, &name).await?;
                tags::attach(txn, &Target::Ws(&ws_id), &[name_tag], true).await?;
                if !user_tags.is_empty() {
                    let extra = tags::ensure_tag_set(txn, &user_tags).await?;
                    tags::attach(txn, &Target::Ws(&ws_id), &extra, false).await?;
                }
                by_id(txn, &ws_id).await
            })
        })
        .await?;

    // 首个实体 ws 自动成为默认
    if meta::get(db, meta::DEFAULT_WS_KEY).await?.is_none() {
        meta::set(db, meta::DEFAULT_WS_KEY, &model.id).await?;
    }
    Ok(model)
}

// ---------- 变更 ----------

pub async fn update_tags(
    db: &DatabaseConnection,
    name: &str,
    mutation: &TagMutation,
) -> AppResult<()> {
    let ws = require_entity_ws(db, name).await?;
    db.transaction(|txn| {
        let mutation = mutation.clone();
        let ws_id = ws.id.clone();
        Box::pin(async move { tags::apply_mutation(txn, &Target::Ws(&ws_id), &mutation).await })
    })
    .await?;
    Ok(())
}

/// 重命名（仅 DB；磁盘目录名不变，name tag 同步更新）
pub async fn rename(db: &DatabaseConnection, old: &str, new: &str) -> AppResult<()> {
    let ws = require_entity_ws(db, old).await?;
    if by_name(db, new).await?.is_some() {
        return Err(AppError::conflict(format!("workspace 名称已存在：{new}")));
    }

    db.transaction(|txn| {
        let ws_id = ws.id.clone();
        let new = new.to_owned();
        Box::pin(async move {
            let now = chrono::Utc::now();
            let mut am: workspace::ActiveModel = by_id(txn, &ws_id).await?.into();
            am.name = Set(new.clone());
            am.updated_at = Set(now);
            am.update(txn).await?;

            // 同步 name tag
            let current = tags::entity_tags(txn, &Target::Ws(&ws_id)).await?;
            if let Some(name_tag) = current.into_iter().find(|t| t.is_name) {
                let mut tag_am: tag::ActiveModel = tag::Entity::find()
                    .filter(tag::Column::Id.eq(name_tag.id))
                    .one(txn)
                    .await?
                    .unwrap()
                    .into();
                tag_am.tag_value = Set(new);
                tag_am.update(txn).await?;
            }
            Ok(())
        })
    })
    .await?;
    Ok(())
}

/// 注销 workspace；purge 时删除磁盘目录
pub async fn remove(db: &DatabaseConnection, name: &str, purge: bool) -> AppResult<()> {
    let ws = by_name(db, name)
        .await?
        .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")))?;
    if ws.is_builtin {
        return Err(AppError::invalid_args(format!(
            "内置 workspace `{}` 不可删除",
            ws.name
        )));
    }

    let pj_count = project::Entity::find()
        .filter(project::Column::WorkspaceId.eq(&ws.id))
        .count(db)
        .await?;
    if pj_count > 0 {
        return Err(AppError::conflict(format!(
            "workspace `{}` 下仍有 {pj_count} 个 project，请先移除或移动它们",
            ws.name
        )));
    }

    if purge {
        if let Some(root) = &ws.root_path {
            let path = std::path::PathBuf::from(root);
            if path.exists() {
                tokio::task::spawn_blocking(move || std::fs::remove_dir_all(path)).await??;
            }
        }
    }

    workspace_tag::Entity::delete_many()
        .filter(workspace_tag::Column::WorkspaceId.eq(&ws.id))
        .exec(db)
        .await?;
    workspace::Entity::delete_many()
        .filter(workspace::Column::Id.eq(&ws.id))
        .exec(db)
        .await?;

    if meta::get(db, meta::DEFAULT_WS_KEY)
        .await?
        .is_some_and(|id| id == ws.id)
    {
        meta::delete(db, meta::DEFAULT_WS_KEY).await?;
    }
    Ok(())
}

/// 设置默认 workspace（必须是实体 ws）
pub async fn set_default(db: &DatabaseConnection, name: &str) -> AppResult<()> {
    let ws = require_entity_ws(db, name).await?;
    meta::set(db, meta::DEFAULT_WS_KEY, &ws.id).await?;
    Ok(())
}

/// jump 目标绝对路径（虚拟/断链报错）
pub async fn jump_path(db: &DatabaseConnection, name: &str) -> AppResult<String> {
    let ws = by_name(db, name)
        .await?
        .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")))?;
    match ws.root_path {
        Some(path) if std::path::Path::new(&path).is_dir() => Ok(path),
        Some(path) => Err(AppError::not_found(format!(
            "workspace `{}` 的目录已不存在（断链）：{path}",
            ws.name
        ))),
        None => Err(AppError::invalid_args(format!(
            "workspace `{}` 是虚拟 workspace，无目录可跳转；请 jump 其下的 project",
            ws.name
        ))),
    }
}

// ---------- 内部辅助 ----------

/// 取实体 ws（拒绝 dvws 等虚拟 ws）
async fn require_entity_ws(db: &DatabaseConnection, name: &str) -> AppResult<workspace::Model> {
    let ws = by_name(db, name)
        .await?
        .ok_or_else(|| AppError::not_found(format!("workspace 不存在：{name}")))?;
    if ws.root_path.is_none() {
        return Err(AppError::invalid_args(format!(
            "workspace `{name}` 是虚拟 workspace，不支持此操作",
        )));
    }
    Ok(ws)
}

/// 确保实体名称对应的 DefaultTags tag 存在
async fn ensure_name_tag<C: ConnectionTrait>(db: &C, name: &str) -> AppResult<tag::Model> {
    let id = tags::ensure_tag(db, crate::domain::tags::DEFAULT_TAGS_KEY, name).await?;
    Ok(tag::Model {
        id,
        tag_key: crate::domain::tags::DEFAULT_TAGS_KEY.to_owned(),
        tag_value: name.to_owned(),
        created_at: chrono::Utc::now(),
    })
}
