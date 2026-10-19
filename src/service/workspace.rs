use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};

use crate::entity::workspace;
use crate::error::AppResult;

/// 列出全部 workspace：内置 dvws 排在最前，其余按名称排序
pub async fn list(db: &DatabaseConnection) -> AppResult<Vec<workspace::Model>> {
    let rows = workspace::Entity::find()
        .order_by_desc(workspace::Column::IsBuiltin)
        .order_by_asc(workspace::Column::Name)
        .all(db)
        .await?;
    Ok(rows)
}
