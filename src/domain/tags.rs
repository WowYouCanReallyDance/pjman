//! tag 语法解析（纯函数，无 IO）
//!
//! 语法：
//! - `key=value`：单值
//! - `key=value1,value2,value3`：同 key 多值（key 内 OR）
//! - `OpenSrc`：裸值，归入默认 key [`DEFAULT_TAGS_KEY`]
//! - 值中的逗号/等号/反斜杠用 `\,`、`\=`、`\\` 转义
//! - 同一 key 多次出现（含重复 `--tags`）取并集

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{AppError, AppResult};

/// 裸值归属的默认 key；ws/pj 的 name 也是该 key 下的首值
pub const DEFAULT_TAGS_KEY: &str = "DefaultTags";

/// 一个 token 解析结果：单个 key 与一到多个 value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTag {
    pub key: String,
    pub values: Vec<String>,
}

/// key -> values 集合
pub type TagSet = BTreeMap<String, BTreeSet<String>>;

/// 解析 add/select/`--tags` 场景的一个 token（裸值归入 DefaultTags）
pub fn parse_tag_token(token: &str) -> AppResult<ParsedTag> {
    let (raw_key, raw_values) = split_key_values(token)?;
    let key = match raw_key {
        Some(key) => {
            validate_key(&key)?;
            key
        }
        None => DEFAULT_TAGS_KEY.to_owned(),
    };
    let values: Vec<String> = raw_values.into_iter().filter(|v| !v.is_empty()).collect();
    if values.is_empty() {
        return Err(AppError::invalid_args(format!("tag 缺少 value：{token}")));
    }
    Ok(ParsedTag { key, values })
}

/// 解析 `tg rm` / `up --remove` 场景的 token：裸值表示 key 本身。
/// 返回 (key, 可选的 value 列表)
pub fn parse_key_opt_values(token: &str) -> AppResult<(String, Option<Vec<String>>)> {
    let (raw_key, raw_values) = split_key_values(token)?;
    let (key, values) = match raw_key {
        // `key=v1,v2`
        Some(key) => {
            let values: Vec<String> = raw_values.into_iter().filter(|v| !v.is_empty()).collect();
            (
                key,
                if values.is_empty() {
                    None
                } else {
                    Some(values)
                },
            )
        }
        // 裸 token：整体作为 key，无 value
        None => {
            let key = raw_values.into_iter().next().unwrap_or_default();
            (key, None)
        }
    };
    if key.is_empty() {
        return Err(AppError::invalid_args(format!(
            "删除 tag 需要指定 key：{token}"
        )));
    }
    validate_key(&key)?;
    Ok((key, values))
}

/// 将多个 token 合并为 TagSet（同 key 取并集）
pub fn tokens_to_set(tokens: &[String]) -> AppResult<TagSet> {
    let mut set = TagSet::new();
    for token in tokens {
        let parsed = parse_tag_token(token)?;
        let entry = set.entry(parsed.key).or_default();
        entry.extend(parsed.values);
    }
    Ok(set)
}

/// key 校验：字母或下划线开头，仅含字母数字、下划线、点、连字符
fn validate_key(key: &str) -> AppResult<()> {
    let mut chars = key.chars();
    let valid_first = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let valid_rest = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if valid_first && valid_rest {
        Ok(())
    } else {
        Err(AppError::invalid_args(format!(
            "非法 tag key `{key}`：需以字母或下划线开头，仅含字母数字、_、.、-"
        )))
    }
}

/// 在识别转义的前提下切出 (key, values)；无 `=` 时 key 为 None
fn split_key_values(token: &str) -> AppResult<(Option<String>, Vec<String>)> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut saw_eq = false;
    let mut comma_before_eq = false;
    let mut chars = token.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let next = chars.next().ok_or_else(|| {
                    AppError::invalid_args(format!("非法转义（反斜杠结尾）：{token}"))
                })?;
                current.push(next);
            }
            '=' if !saw_eq => {
                saw_eq = true;
                segments.push(std::mem::take(&mut current));
            }
            '=' => current.push('='),
            ',' => {
                // 有 '=' 时是 value 分隔；无 '=' 时是裸值分隔；
                // 只有逗号出现在 key 与第一个 '=' 之间才非法（在 saw_eq 置位后排除）
                if saw_eq {
                    segments.push(std::mem::take(&mut current));
                } else {
                    comma_before_eq = true;
                    segments.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    segments.push(current);

    if saw_eq && comma_before_eq {
        return Err(AppError::invalid_args(format!(
            "tag key 中不能包含逗号：{token}"
        )));
    }

    let mut iter = segments.into_iter();
    let first = iter.next().unwrap_or_default();
    let rest: Vec<String> = iter.collect();
    if saw_eq {
        Ok((Some(first), rest))
    } else {
        // 无 '='：各段均为 DefaultTags 下的裸值
        let mut values = vec![first];
        values.extend(rest);
        Ok((None, values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_kv() {
        let tag = parse_tag_token("License=MIT").unwrap();
        assert_eq!(tag.key, "License");
        assert_eq!(tag.values, vec!["MIT"]);
    }

    #[test]
    fn multi_values() {
        let tag = parse_tag_token("License=MIT,BSD3.0,GNUv3.0").unwrap();
        assert_eq!(tag.values, vec!["MIT", "BSD3.0", "GNUv3.0"]);
    }

    #[test]
    fn bare_value_goes_to_default() {
        let tag = parse_tag_token("OpenSrc").unwrap();
        assert_eq!(tag.key, DEFAULT_TAGS_KEY);
        assert_eq!(tag.values, vec!["OpenSrc"]);
    }

    #[test]
    fn escaped_comma_in_value() {
        let tag = parse_tag_token(r"Authors=Matt Long\,Taylor Swift").unwrap();
        assert_eq!(tag.key, "Authors");
        assert_eq!(tag.values, vec!["Matt Long,Taylor Swift"]);
    }

    #[test]
    fn escaped_equals_and_backslash() {
        let tag = parse_tag_token(r"k=a\=b\\c").unwrap();
        assert_eq!(tag.values, vec!["a=b\\c"]);
    }

    #[test]
    fn repeated_tokens_union() {
        let set = tokens_to_set(&[
            "Authors=Matt Long".to_owned(),
            "Authors=Taylor Swift".to_owned(),
            "License=MIT".to_owned(),
        ])
        .unwrap();
        assert_eq!(set["Authors"].len(), 2);
        assert!(set["Authors"].contains("Matt Long"));
        assert!(set["License"].contains("MIT"));
    }

    #[test]
    fn invalid_keys() {
        assert!(parse_tag_token("1bad=x").is_err());
        assert!(parse_tag_token("bad key=x").is_err());
        assert!(parse_tag_token("bad/key=x").is_err());
        assert!(parse_tag_token("=x").is_err());
    }

    #[test]
    fn dangling_backslash_errors() {
        assert!(parse_tag_token(r"k=a\").is_err());
    }

    #[test]
    fn key_opt_values_parsing() {
        assert_eq!(
            parse_key_opt_values("License").unwrap(),
            ("License".to_owned(), None)
        );
        let (key, values) = parse_key_opt_values("License=MIT,BSD").unwrap();
        assert_eq!(key, "License");
        assert_eq!(values.unwrap(), vec!["MIT", "BSD"]);
        // 裸 token 在删除语义下是 key 本身
        assert_eq!(
            parse_key_opt_values("OpenSrc").unwrap(),
            ("OpenSrc".to_owned(), None)
        );
        assert!(parse_key_opt_values("").is_err());
    }

    #[test]
    fn bare_multi_values() {
        let tag = parse_tag_token("OpenSrc,Apache").unwrap();
        assert_eq!(tag.key, DEFAULT_TAGS_KEY);
        assert_eq!(tag.values, vec!["OpenSrc", "Apache"]);
    }

    #[test]
    fn comma_in_key_position_errors() {
        assert!(parse_tag_token("a,b=x").is_err());
    }
}
