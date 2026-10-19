//! meta 单例 KV（默认 workspace 指针等）

use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    sea_query::OnConflict,
};

use crate::entity::meta;
use crate::error::AppResult;

pub const DEFAULT_WS_KEY: &str = "default_ws_id";

pub async fn get(db: &DatabaseConnection, key: &str) -> AppResult<Option<String>> {
    Ok(meta::Entity::find()
        .filter(meta::Column::Key.eq(key))
        .one(db)
        .await?
        .map(|m| m.value))
}

pub async fn delete(db: &DatabaseConnection, key: &str) -> AppResult<()> {
    meta::Entity::delete_many()
        .filter(meta::Column::Key.eq(key))
        .exec(db)
        .await?;
    Ok(())
}

pub async fn set(db: &DatabaseConnection, key: &str, value: &str) -> AppResult<()> {
    meta::Entity::insert(meta::ActiveModel {
        key: Set(key.to_owned()),
        value: Set(value.to_owned()),
    })
    .on_conflict(
        OnConflict::column(meta::Column::Key)
            .update_column(meta::Column::Value)
            .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}
