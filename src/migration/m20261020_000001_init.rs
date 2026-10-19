use sea_orm::{EntityTrait, Set};
use sea_orm_migration::prelude::*;

use crate::domain::ids;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Meta {
    Table,
    Key,
    Value,
}

#[derive(DeriveIden)]
enum Workspace {
    #[sea_orm(iden = "workspaces")]
    Table,
    Id,
    Name,
    RootPath,
    IsBuiltin,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Project {
    #[sea_orm(iden = "projects")]
    Table,
    Id,
    WorkspaceId,
    Name,
    Path,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Tag {
    #[sea_orm(iden = "tags")]
    Table,
    Id,
    #[sea_orm(iden = "tag_key")]
    Key,
    #[sea_orm(iden = "tag_value")]
    Value,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WorkspaceTag {
    #[sea_orm(iden = "workspace_tags")]
    Table,
    WorkspaceId,
    TagId,
    IsName,
    CreatedAt,
}

#[derive(DeriveIden)]
enum ProjectTag {
    #[sea_orm(iden = "project_tags")]
    Table,
    ProjectId,
    TagId,
    IsName,
    CreatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // meta：单例 KV（默认 ws 指针等）
        manager
            .create_table(
                Table::create()
                    .table(Meta::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Meta::Key).string().not_null().primary_key())
                    .col(ColumnDef::new(Meta::Value).string().not_null())
                    .to_owned(),
            )
            .await?;

        // workspaces
        manager
            .create_table(
                Table::create()
                    .table(Workspace::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Workspace::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Workspace::Name)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Workspace::RootPath).string())
                    .col(
                        ColumnDef::new(Workspace::IsBuiltin)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Workspace::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Workspace::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // tags：全局可用 tag 字典
        manager
            .create_table(
                Table::create()
                    .table(Tag::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Tag::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Tag::Key).string().not_null())
                    .col(ColumnDef::new(Tag::Value).string().not_null())
                    .col(
                        ColumnDef::new(Tag::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ux_tags_key_value")
                    .table(Tag::Table)
                    .unique()
                    .col(Tag::Key)
                    .col(Tag::Value)
                    .to_owned(),
            )
            .await?;

        // projects
        manager
            .create_table(
                Table::create()
                    .table(Project::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Project::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Project::WorkspaceId).string().not_null())
                    .col(ColumnDef::new(Project::Name).string().not_null())
                    .col(
                        ColumnDef::new(Project::Path)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Project::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Project::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_projects_workspace")
                            .from(Project::Table, Project::WorkspaceId)
                            .to(Workspace::Table, Workspace::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("ux_projects_workspace_name")
                    .table(Project::Table)
                    .unique()
                    .col(Project::WorkspaceId)
                    .col(Project::Name)
                    .to_owned(),
            )
            .await?;

        // workspace_tags
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceTag::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkspaceTag::WorkspaceId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkspaceTag::TagId).string().not_null())
                    .col(
                        ColumnDef::new(WorkspaceTag::IsName)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WorkspaceTag::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(WorkspaceTag::WorkspaceId)
                            .col(WorkspaceTag::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_tags_workspace")
                            .from(WorkspaceTag::Table, WorkspaceTag::WorkspaceId)
                            .to(Workspace::Table, Workspace::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_workspace_tags_tag")
                            .from(WorkspaceTag::Table, WorkspaceTag::TagId)
                            .to(Tag::Table, Tag::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_workspace_tags_tag")
                    .table(WorkspaceTag::Table)
                    .col(WorkspaceTag::TagId)
                    .to_owned(),
            )
            .await?;

        // project_tags
        manager
            .create_table(
                Table::create()
                    .table(ProjectTag::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ProjectTag::ProjectId).string().not_null())
                    .col(ColumnDef::new(ProjectTag::TagId).string().not_null())
                    .col(
                        ColumnDef::new(ProjectTag::IsName)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(ProjectTag::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(ProjectTag::ProjectId)
                            .col(ProjectTag::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_project_tags_project")
                            .from(ProjectTag::Table, ProjectTag::ProjectId)
                            .to(Project::Table, Project::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_project_tags_tag")
                            .from(ProjectTag::Table, ProjectTag::TagId)
                            .to(Tag::Table, Tag::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_project_tags_tag")
                    .table(ProjectTag::Table)
                    .col(ProjectTag::TagId)
                    .to_owned(),
            )
            .await?;

        seed_dvws(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ProjectTag::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WorkspaceTag::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Project::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Tag::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Workspace::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Meta::Table).to_owned())
            .await?;
        Ok(())
    }
}

/// 种子：内置默认虚拟 workspace dvws（无实体目录、不可删除）
async fn seed_dvws(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let now = chrono::Utc::now();
    crate::entity::workspace::Entity::insert(crate::entity::workspace::ActiveModel {
        id: Set(ids::DVWS_ID.clone()),
        name: Set(ids::DVWS_NAME.to_owned()),
        root_path: Set(None),
        is_builtin: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .on_conflict_do_nothing()
    .exec(manager.get_connection())
    .await?;
    Ok(())
}
