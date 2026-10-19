//! 跨平台路径工具（纯文件系统辅助）

use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};

/// 已存在则 canonicalize（解析符号链接）；否则相对 cwd 绝对化并词法规范化
pub fn resolve(path: &Path) -> AppResult<PathBuf> {
    if path.exists() {
        return Ok(std::fs::canonicalize(path)?);
    }
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize(&abs))
}

/// 词法规范化（消除 `.`/`..`，不触碰文件系统、不解析符号链接）
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 判断 `child` 是否位于 `root` 目录之内（规范化后比较前缀）
/// 阶段 2 备份去重使用
#[allow(dead_code)]
pub fn within(root: &Path, child: &Path) -> bool {
    let root = normalize(root);
    let child = normalize(child);
    child.starts_with(&root)
}

/// 取路径的文件名（UTF-8）
pub fn file_name(path: &Path) -> AppResult<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .ok_or_else(|| AppError::invalid_args(format!("路径名非法：{}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_dot_segments() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
    }

    #[test]
    fn within_check() {
        assert!(within(Path::new("/a/b"), Path::new("/a/b/c/d")));
        assert!(!within(Path::new("/a/b"), Path::new("/a/bad")));
        assert!(!within(Path::new("/a/b"), Path::new("/a/../c/b/x")));
    }
}
