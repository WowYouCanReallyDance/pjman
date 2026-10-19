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
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("路径包含非法字符（非 UTF-8）：{0}")]
    InvalidPath(String),

    #[error("无法确定用户主目录，请通过 --home 或 PJMAN_HOME 指定")]
    NoHomeDir,

    #[error("{0}")]
    InvalidArgs(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{message}")]
    Ambiguous {
        message: String,
        /// 可用于收敛歧义的 `DefaultTags=<名称>` 候选
        candidates: Vec<String>,
    },

    #[error("命令尚未实现：{0}")]
    Unimplemented(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// 事务错误收敛：连接错误归 Db，回调错误归回 AppError 自身
impl From<sea_orm::TransactionError<AppError>> for AppError {
    fn from(err: sea_orm::TransactionError<AppError>) -> Self {
        match err {
            sea_orm::TransactionError::Connection(db_err) => Self::Db(db_err),
            sea_orm::TransactionError::Transaction(app_err) => app_err,
        }
    }
}

impl AppError {
    pub fn invalid_args(msg: impl Into<String>) -> Self {
        Self::InvalidArgs(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    pub fn ambiguous(message: impl Into<String>, candidates: Vec<String>) -> Self {
        Self::Ambiguous {
            message: message.into(),
            candidates,
        }
    }
}
