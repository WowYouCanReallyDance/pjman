//! `pjman __complete <kind> [partial...]`：供 shell 钩子查询动态补全候选。
//! 每行一个候选；候选随最后一个 partial token 做前缀过滤。

use std::collections::BTreeSet;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::app::AppCtx;
use crate::cli::CompleteKind;
use crate::entity::{project, tag, workspace};
use crate::error::AppResult;

pub async fn run(ctx: &AppCtx, kind: CompleteKind, partial: Vec<String>) -> AppResult<()> {
    let prefix = partial.last().cloned().unwrap_or_default();
    let candidates = match kind {
        CompleteKind::Ws => workspace::Entity::find()
            .all(&ctx.db)
            .await?
            .into_iter()
            .map(|m| m.name)
            .filter(|n| n.starts_with(&prefix))
            .collect::<BTreeSet<_>>(),
        CompleteKind::Pj => project::Entity::find()
            .all(&ctx.db)
            .await?
            .into_iter()
            .map(|m| m.name)
            .filter(|n| n.starts_with(&prefix))
            .collect::<BTreeSet<_>>(),
        CompleteKind::TagKey => tag::Entity::find()
            .all(&ctx.db)
            .await?
            .into_iter()
            .map(|t| t.tag_key)
            .filter(|k| k.starts_with(&prefix))
            .collect::<BTreeSet<_>>(),
        CompleteKind::TagValue => {
            // key=前缀时输出 key=value 候选
            if let Some((key, value_prefix)) = prefix.split_once('=') {
                tag::Entity::find()
                    .filter(tag::Column::TagKey.eq(key))
                    .all(&ctx.db)
                    .await?
                    .into_iter()
                    .map(|t| format!("{}={}", t.tag_key, t.tag_value))
                    .filter(|c| c.starts_with(&format!("{key}={value_prefix}")))
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::new()
            }
        }
    };

    for c in candidates {
        println!("{c}");
    }
    Ok(())
}
