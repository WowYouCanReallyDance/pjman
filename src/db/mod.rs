pub mod meta;

use std::time::Duration;

use sea_orm::sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

use crate::config::Config;
use crate::error::AppResult;
use crate::migration::Migrator;

/// 建立数据库连接并执行待应用的迁移。
///
/// SQLite 使用专用建连路径，以便在连接池的每个连接上统一设置 PRAGMA
/// （外键、WAL、忙等超时、同步级别）；MySQL/PostgreSQL 走通用 URL 建连。
pub async fn connect(cfg: &Config) -> AppResult<DatabaseConnection> {
    let db = if cfg.database_url.starts_with("sqlite:") {
        let options: SqliteConnectOptions = cfg.database_url.parse()?;
        let options = options
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5))
            .synchronous(SqliteSynchronous::Normal);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool)
    } else {
        let mut options = ConnectOptions::new(cfg.database_url.clone());
        options.sqlx_logging(false);
        Database::connect(options).await?
    };

    Migrator::up(&db, None).await?;
    Ok(db)
}
