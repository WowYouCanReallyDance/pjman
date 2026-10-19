use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),

    #[error(transparent)]
    Sqlx(#[from] sea_orm::sqlx::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("路径包含非法字符（非 UTF-8）：{0}")]
    InvalidPath(String),

    #[error("无法确定用户主目录，请通过 --home 或 PJMAN_HOME 指定")]
    NoHomeDir,

    #[error("命令尚未实现：{0}")]
    Unimplemented(String),
}

pub type AppResult<T> = Result<T, AppError>;
