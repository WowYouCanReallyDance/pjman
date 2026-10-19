use sea_orm::DatabaseConnection;

use crate::config::Config;

pub struct AppCtx {
    pub db: DatabaseConnection,
    /// 阶段 2 备份/恢复等功能使用
    #[allow(dead_code)]
    pub cfg: Config,
    pub json: bool,
    /// 阶段 1 的删除等确认提示使用
    #[allow(dead_code)]
    pub yes: bool,
}
