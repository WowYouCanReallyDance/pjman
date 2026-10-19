// SeaORM 实体是数据库 schema 的映射，字段/Relation 由 ORM 使用，
// 即使当前命令尚未读取也不视为死代码
#![allow(dead_code)]

pub mod meta;
pub mod project;
pub mod project_tag;
pub mod tag;
pub mod workspace;
pub mod workspace_tag;
