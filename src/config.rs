use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    /// pjman 数据目录（SQLite 文件所在），默认 ~/.pjman；备份等功能使用
    #[allow(dead_code)]
    pub home: PathBuf,
    /// 数据库连接串
    pub database_url: String,
}

impl Config {
    /// 解析优先级：命令行参数 > 环境变量 > 默认值
    pub fn resolve(home_flag: Option<PathBuf>, url_flag: Option<String>) -> AppResult<Self> {
        let home = match home_flag {
            Some(home) => home,
            None => match std::env::var_os("PJMAN_HOME") {
                Some(home) => PathBuf::from(home),
                None => {
                    let mut dir = dirs::home_dir().ok_or(AppError::NoHomeDir)?;
                    dir.push(".pjman");
                    dir
                }
            },
        };
        std::fs::create_dir_all(&home)?;
        // 规范化为绝对路径，保证 sqlite URL 拼接正确（相对路径会被误判为 host）
        let home = std::fs::canonicalize(home)?;

        let database_url = match url_flag {
            Some(url) => url,
            None => match std::env::var("PJMAN_DATABASE_URL") {
                Ok(url) => url,
                Err(_) => default_sqlite_url(&home.join("pjman.sqlite"))?,
            },
        };

        Ok(Self { home, database_url })
    }
}

/// 构造 SQLx 可解析的 sqlite URL，并为连接池中的每个连接设置 PRAGMA。
fn default_sqlite_url(path: &Path) -> AppResult<String> {
    let path_str = path
        .to_str()
        .ok_or_else(|| AppError::InvalidPath(path.display().to_string()))?;
    let mut encoded = String::new();
    for &byte in path_str.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    // PRAGMA 由 db 模块在 SqliteConnectOptions 上为每个连接设置
    // （SQLx 0.9 已移除 URL 的 _pragma 参数）
    Ok(format!("sqlite://{encoded}?mode=rwc"))
}
