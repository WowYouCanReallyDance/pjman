//! tag 字典与实体 tag 关联的共享操作

use std::collections::{BTreeMap, BTreeSet};

use sea_orm::{
    ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, sea_query::OnConflict,
};

use crate::domain::{
    ids,
    selector::{filter_entities, group_matched_keys},
    tags::TagSet,
};
use crate::entity::{project_tag, tag, workspace_tag};
use crate::error::{AppError, AppResult};

/// tag 挂载的实体目标
pub enum Target<'a> {
    Ws(&'a str),
    Pj(&'a str),
}

#[derive(Debug, Clone)]
pub struct EntityTag {
    pub id: String,
    pub key: String,
    pub value: String,
    pub is_name: bool,
}

/// 一次 `up` 调用的全部 tag 变更
#[derive(Clone)]
pub struct TagMutation {
    pub add: TagSet,
    pub set: TagSet,
    pub remove: Vec<(String, Option<Vec<String>>)>,
}

impl TagMutation {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.set.is_empty() && self.remove.is_empty()
    }
}

// ---------- 字典 ----------

/// 取字典 tag，不存在则插入（幂等）
pub async fn ensure_tag<C: ConnectionTrait>(db: &C, key: &str, value: &str) -> AppResult<String> {
    if let Some(existing) = tag::Entity::find()
        .filter(tag::Column::TagKey.eq(key))
        .filter(tag::Column::TagValue.eq(value))
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }
    let id = ids::new_id();
    tag::Entity::insert(tag::ActiveModel {
        id: Set(id.clone()),
        tag_key: Set(key.to_owned()),
        tag_value: Set(value.to_owned()),
        created_at: Set(chrono::Utc::now()),
    })
    .on_conflict(
        OnConflict::columns([tag::Column::TagKey, tag::Column::TagValue])
            .do_nothing()
            .to_owned(),
    )
    .exec(db)
    .await?;
    let row = tag::Entity::find()
        .filter(tag::Column::TagKey.eq(key))
        .filter(tag::Column::TagValue.eq(value))
        .one(db)
        .await?
        .ok_or_else(|| AppError::conflict(format!("tag {key}={value} 写入失败")))?;
    Ok(row.id)
}

/// 确保 TagSet 中所有字典项存在，返回对应实体（顺序不保证）
pub async fn ensure_tag_set<C: ConnectionTrait>(
    db: &C,
    set: &TagSet,
) -> AppResult<Vec<tag::Model>> {
    let mut models = Vec::new();
    for (key, values) in set {
        for value in values {
            let id = ensure_tag(db, key, value).await?;
            models.push(tag::Model {
                id,
                tag_key: key.clone(),
                tag_value: value.clone(),
                created_at: chrono::Utc::now(),
            });
        }
    }
    Ok(models)
}

pub async fn dict_list<C: ConnectionTrait>(
    db: &C,
    key_filter: Option<&str>,
) -> AppResult<Vec<tag::Model>> {
    let mut query = tag::Entity::find();
    if let Some(key) = key_filter {
        query = query.filter(tag::Column::TagKey.eq(key));
    }
    let mut rows = query
        .order_by_asc(tag::Column::TagKey)
        .order_by_asc(tag::Column::TagValue)
        .all(db)
        .await?;
    rows.sort_by(|a, b| {
        a.tag_key
            .cmp(&b.tag_key)
            .then_with(|| a.tag_value.cmp(&b.tag_value))
    });
    Ok(rows)
}

/// 字典项被多少 ws/pj 引用
pub async fn reference_count<C: ConnectionTrait>(db: &C, tag_id: &str) -> AppResult<u64> {
    let ws = workspace_tag::Entity::find()
        .filter(workspace_tag::Column::TagId.eq(tag_id))
        .count(db)
        .await?;
    let pj = project_tag::Entity::find()
        .filter(project_tag::Column::TagId.eq(tag_id))
        .count(db)
        .await?;
    Ok(ws + pj)
}

/// 从字典删除 tag；被引用时默认拒绝，force 则连带删除实体关联
/// 返回被删除的 `key=value` 描述
pub async fn dict_remove<C: ConnectionTrait>(
    db: &C,
    specs: Vec<(String, Option<Vec<String>>)>,
    force: bool,
) -> AppResult<Vec<String>> {
    let mut targets: Vec<tag::Model> = Vec::new();
    for (key, values) in &specs {
        let mut query = tag::Entity::find().filter(tag::Column::TagKey.eq(key.clone()));
        if let Some(values) = values {
            query = query.filter(tag::Column::TagValue.is_in(values.clone()));
        }
        let rows = query.all(db).await?;
        if rows.is_empty() {
            return Err(AppError::not_found(format!(
                "字典中不存在 tag：{key}{}",
                match values {
                    Some(v) => format!("={}", v.join(",")),
                    None => String::new(),
                }
            )));
        }
        targets.extend(rows);
    }

    for t in &targets {
        // name tag 永远受保护（即使 --force）
        let used_as_name = workspace_tag::Entity::find()
            .filter(workspace_tag::Column::TagId.eq(&t.id))
            .filter(workspace_tag::Column::IsName.eq(true))
            .count(db)
            .await?
            + project_tag::Entity::find()
                .filter(project_tag::Column::TagId.eq(&t.id))
                .filter(project_tag::Column::IsName.eq(true))
                .count(db)
                .await?;
        if used_as_name > 0 {
            return Err(AppError::conflict(format!(
                "tag `{}={}` 是实体名称，受保护，不能从字典删除",
                t.tag_key, t.tag_value
            )));
        }
        let count = reference_count(db, &t.id).await?;
        if count > 0 && !force {
            return Err(AppError::conflict(format!(
                "tag `{}={}` 正被 {count} 个 workspace/project 引用；加 --force 可连同赋值一起删除",
                t.tag_key, t.tag_value
            )));
        }
    }

    let ids: Vec<String> = targets.iter().map(|t| t.id.clone()).collect();
    if force {
        workspace_tag::Entity::delete_many()
            .filter(workspace_tag::Column::TagId.is_in(ids.clone()))
            .exec(db)
            .await?;
        project_tag::Entity::delete_many()
            .filter(project_tag::Column::TagId.is_in(ids.clone()))
            .exec(db)
            .await?;
    }
    tag::Entity::delete_many()
        .filter(tag::Column::Id.is_in(ids))
        .exec(db)
        .await?;

    Ok(targets
        .into_iter()
        .map(|t| format!("{}={}", t.tag_key, t.tag_value))
        .collect())
}

/// 按选择器（key 内 OR、key 间 AND）返回命中的实体 id 列表
pub async fn matching_ids<C: ConnectionTrait>(
    db: &C,
    kind: TargetKind,
    required: &TagSet,
) -> AppResult<Vec<String>> {
    if required.is_empty() {
        return Ok(Vec::new());
    }

    // 取字典中满足任一 (key,value) 条件的 tag
    let all_tags = tag::Entity::find().all(db).await?;
    let matched_tag_ids: BTreeSet<String> = all_tags
        .iter()
        .filter(|t| {
            required
                .get(&t.tag_key)
                .is_some_and(|values| values.contains(&t.tag_value))
        })
        .map(|t| t.id.clone())
        .collect();

    if matched_tag_ids.is_empty() {
        return Ok(Vec::new());
    }

    let hits: Vec<(String, String)> = match kind {
        TargetKind::Ws => workspace_tag::Entity::find()
            .filter(
                workspace_tag::Column::TagId.is_in(matched_tag_ids.into_iter().collect::<Vec<_>>()),
            )
            .all(db)
            .await?
            .into_iter()
            .filter_map(|row| {
                all_tags
                    .iter()
                    .find(|t| t.id == row.tag_id)
                    .map(|t| (row.workspace_id, t.tag_key.clone()))
            })
            .collect(),
        TargetKind::Pj => project_tag::Entity::find()
            .filter(
                project_tag::Column::TagId
                    .is_in(matched_tag_ids.iter().cloned().collect::<Vec<_>>()),
            )
            .all(db)
            .await?
            .into_iter()
            .filter_map(|row| {
                all_tags
                    .iter()
                    .find(|t| t.id == row.tag_id)
                    .map(|t| (row.project_id, t.tag_key.clone()))
            })
            .collect(),
    };

    let grouped = group_matched_keys(hits);
    let entity_ids: Vec<String> = grouped.keys().cloned().collect();
    Ok(filter_entities(entity_ids, required, |id| {
        grouped.get(id).cloned().unwrap_or_default()
    }))
}

// ---------- 实体关联 ----------

/// 将 tags 关联到目标实体（已有关联跳过）
pub async fn attach<C: ConnectionTrait>(
    db: &C,
    target: &Target<'_>,
    tags: &[tag::Model],
    is_name: bool,
) -> AppResult<()> {
    let now = chrono::Utc::now();
    for t in tags {
        attach_one(db, target, &t.id, is_name, now).await?;
    }
    Ok(())
}

async fn attach_one<C: ConnectionTrait>(
    db: &C,
    target: &Target<'_>,
    tag_id: &str,
    is_name: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> AppResult<()> {
    match target {
        Target::Ws(ws_id) => {
            workspace_tag::Entity::insert(workspace_tag::ActiveModel {
                workspace_id: Set((*ws_id).to_owned()),
                tag_id: Set(tag_id.to_owned()),
                is_name: Set(is_name),
                created_at: Set(now),
            })
            .on_conflict(
                OnConflict::columns([
                    workspace_tag::Column::WorkspaceId,
                    workspace_tag::Column::TagId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(db)
            .await?;
        }
        Target::Pj(pj_id) => {
            project_tag::Entity::insert(project_tag::ActiveModel {
                project_id: Set((*pj_id).to_owned()),
                tag_id: Set(tag_id.to_owned()),
                is_name: Set(is_name),
                created_at: Set(now),
            })
            .on_conflict(
                OnConflict::columns([project_tag::Column::ProjectId, project_tag::Column::TagId])
                    .do_nothing()
                    .to_owned(),
            )
            .exec(db)
            .await?;
        }
    }
    Ok(())
}

/// 目标实体的全部 tag（含 is_name 标记）
pub async fn entity_tags<C: ConnectionTrait>(
    db: &C,
    target: &Target<'_>,
) -> AppResult<Vec<EntityTag>> {
    let id = match target {
        Target::Ws(id) | Target::Pj(id) => (*id).to_owned(),
    };
    let mut map = tags_of_entities(db, target.kind(), std::slice::from_ref(&id)).await?;
    Ok(map.remove(&id).unwrap_or_default())
}

/// 批量取多个实体的 tag
pub async fn tags_of_entities<C: ConnectionTrait>(
    db: &C,
    kind: TargetKind,
    entity_ids: &[String],
) -> AppResult<BTreeMap<String, Vec<EntityTag>>> {
    if entity_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    // 按实体维度取关联（用于分组）
    let mut grouped: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    match kind {
        TargetKind::Ws => {
            for row in workspace_tag::Entity::find()
                .filter(workspace_tag::Column::WorkspaceId.is_in(entity_ids.to_owned()))
                .all(db)
                .await?
            {
                grouped
                    .entry(row.workspace_id)
                    .or_default()
                    .push((row.tag_id, row.is_name));
            }
        }
        TargetKind::Pj => {
            for row in project_tag::Entity::find()
                .filter(project_tag::Column::ProjectId.is_in(entity_ids.to_owned()))
                .all(db)
                .await?
            {
                grouped
                    .entry(row.project_id)
                    .or_default()
                    .push((row.tag_id, row.is_name));
            }
        }
    }

    let all_tag_ids: Vec<String> = grouped
        .values()
        .flat_map(|pairs| pairs.iter().map(|(id, _)| id.clone()))
        .collect();
    let tag_rows = tag::Entity::find()
        .filter(tag::Column::Id.is_in(all_tag_ids))
        .all(db)
        .await?;
    let tag_by_id: BTreeMap<String, tag::Model> =
        tag_rows.into_iter().map(|t| (t.id.clone(), t)).collect();

    let mut out: BTreeMap<String, Vec<EntityTag>> = BTreeMap::new();
    for (entity_id, pairs) in grouped {
        let mut list: Vec<EntityTag> = pairs
            .into_iter()
            .filter_map(|(tag_id, is_name)| {
                tag_by_id.get(&tag_id).map(|t| EntityTag {
                    id: t.id.clone(),
                    key: t.tag_key.clone(),
                    value: t.tag_value.clone(),
                    is_name,
                })
            })
            .collect();
        list.sort_by(|a, b| a.key.cmp(&b.key).then_with(|| a.value.cmp(&b.value)));
        out.insert(entity_id, list);
    }
    Ok(out)
}

#[derive(Clone, Copy)]
pub enum TargetKind {
    Ws,
    Pj,
}

impl Target<'_> {
    pub fn kind(&self) -> TargetKind {
        match self {
            Target::Ws(_) => TargetKind::Ws,
            Target::Pj(_) => TargetKind::Pj,
        }
    }
}

/// 删除指定实体与若干 tag 的关联
async fn detach<C: ConnectionTrait>(
    db: &C,
    target: &Target<'_>,
    tag_ids: &[String],
) -> AppResult<()> {
    if tag_ids.is_empty() {
        return Ok(());
    }
    match target {
        Target::Ws(id) => {
            workspace_tag::Entity::delete_many()
                .filter(workspace_tag::Column::WorkspaceId.eq(*id))
                .filter(workspace_tag::Column::TagId.is_in(tag_ids.to_owned()))
                .exec(db)
                .await?;
        }
        Target::Pj(id) => {
            project_tag::Entity::delete_many()
                .filter(project_tag::Column::ProjectId.eq(*id))
                .filter(project_tag::Column::TagId.is_in(tag_ids.to_owned()))
                .exec(db)
                .await?;
        }
    }
    Ok(())
}

/// 对实体应用 add/set/remove 变更；name 对应的 DefaultTags 受保护
pub async fn apply_mutation<C: ConnectionTrait>(
    db: &C,
    target: &Target<'_>,
    mutation: &TagMutation,
) -> AppResult<()> {
    let current = entity_tags(db, target).await?;

    // 先校验：不允许删除/覆盖 name tag
    for (key, values) in &mutation.remove {
        if let Some(name_tag) = current.iter().find(|t| {
            t.is_name && &t.key == key && values.as_ref().is_none_or(|v| v.contains(&t.value))
        }) {
            return Err(AppError::invalid_args(format!(
                "tag `{}={}` 是实体名称，受保护，请使用 rename 修改",
                name_tag.key, name_tag.value
            )));
        }
    }
    for key in mutation.set.keys() {
        if current.iter().any(|t| t.is_name && &t.key == key) {
            return Err(AppError::invalid_args(format!(
                "key `{key}` 包含受保护的实体名称 tag，请使用 rename 修改"
            )));
        }
    }

    // --set：先移除该 key 下所有非 name 关联，再写入新值
    for key in mutation.set.keys() {
        let values = &mutation.set[key];
        let old: Vec<String> = current
            .iter()
            .filter(|t| &t.key == key && !t.is_name)
            .map(|t| t.id.clone())
            .collect();
        detach(db, target, &old).await?;
        let set_one: TagSet = BTreeMap::from([(key.clone(), values.clone())]);
        let models = ensure_tag_set(db, &set_one).await?;
        attach(db, target, &models, false).await?;
    }

    // --add
    if !mutation.add.is_empty() {
        let models = ensure_tag_set(db, &mutation.add).await?;
        let existing: BTreeSet<String> = current.iter().map(|t| t.id.clone()).collect();
        let fresh: Vec<tag::Model> = models
            .into_iter()
            .filter(|m| !existing.contains(&m.id))
            .collect();
        attach(db, target, &fresh, false).await?;
    }

    // --remove
    for (key, values) in &mutation.remove {
        let doomed: Vec<String> = current
            .iter()
            .filter(|t| {
                &t.key == key && !t.is_name && values.as_ref().is_none_or(|v| v.contains(&t.value))
            })
            .map(|t| t.id.clone())
            .collect();
        detach(db, target, &doomed).await?;
    }

    Ok(())
}
