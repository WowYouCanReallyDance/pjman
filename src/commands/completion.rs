use std::collections::BTreeMap;

use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::Cli;

pub fn run(shell: Shell) {
    print!("{}", String::from_utf8_lossy(&generate_filtered(shell)));
}

/// 生成补全脚本，做三项文本级修正：
///
/// 1. clap_complete 4.6 不过滤 `hide = true` 的子命令，剔除 `__complete` 候选；
/// 2. 缩写候选描述统一为 `= <全称>`（无冗余文案，i18n 友好）；
/// 3. zsh/fish 默认按字母排序候选，会把缩写排到全称前面；
///    zsh 用 `_describe -V`（未排序组）、fish 用 `complete -k`（保持定义顺序），
///    使全称与其缩写按脚本中的声明顺序相邻显示。
///
/// bash/powershell/elvish 的排序由 shell 自身控制，无保序接口，维持字母序。
/// 路由分支与函数定义保留（不产生候选）。
pub fn generate_filtered(shell: Shell) -> Vec<u8> {
    let mut cmd = Cli::command();
    let aliases = collect_aliases(&cmd);

    let mut buf = Vec::new();
    generate(shell, &mut cmd, "pjman", &mut buf);
    let text = String::from_utf8_lossy(&buf);

    let mut kept: Vec<String> = text
        .lines()
        .filter(|line| !is_hidden_candidate(shell, line))
        .map(|line| annotate_alias(shell, line, &aliases))
        .map(|line| keep_declared_order(shell, &line))
        .collect();

    if matches!(shell, Shell::Bash) {
        // bash 的子命令候选是 opts="... a b c" 词表（无描述列）：
        // 摘除 __complete 词；缩写保留原词（bash 无法标注描述）
        for line in &mut kept {
            if line.contains("opts=\"") {
                let rebuilt = line
                    .split_whitespace()
                    .filter(|w| *w != "__complete")
                    .collect::<Vec<_>>()
                    .join(" ");
                *line = rebuilt;
            }
        }
    }

    if matches!(shell, Shell::Zsh) {
        // -V（不带组名）使该命令组不排序，候选按 commands 数组声明顺序展示；
        // 注意不能写成 `-V pjman`：group/tag 位置错位会触发 bad substitution
        for line in &mut kept {
            if line.contains("_describe -t commands") {
                *line = line.replace("_describe -t commands", "_describe -V -t commands");
            }
        }
    }

    kept.join("\n").into_bytes()
}

/// fish：命令候选行加 `-k`（--keep-order），保持脚本声明顺序而非字母序
fn keep_declared_order(shell: Shell, line: &str) -> String {
    if matches!(shell, Shell::Fish) && line.contains("-f -a \"") && !line.contains(" -k ") {
        line.replace("-f -a \"", "-f -k -a \"")
    } else {
        line.to_owned()
    }
}

/// 判断某行是否为隐藏命令 `__complete` 的可见候选
fn is_hidden_candidate(shell: Shell, line: &str) -> bool {
    match shell {
        Shell::Zsh => line.trim_start().starts_with("'__complete:"),
        Shell::Fish => line.contains("-a \"__complete\""),
        Shell::PowerShell => line.contains("[CompletionResult]::new('__complete'"),
        Shell::Elvish => line.trim_start().starts_with("cand __complete "),
        _ => false,
    }
}

/// 收集 别名 → 全称 映射；同名别名在不同上下文指向不同全称时放弃标注
fn collect_aliases(cmd: &clap::Command) -> BTreeMap<String, String> {
    fn walk(cmd: &clap::Command, map: &mut BTreeMap<String, String>) {
        for sub in cmd.get_subcommands() {
            for alias in sub.get_visible_aliases() {
                let canonical = sub.get_name();
                map.entry(alias.to_string())
                    .and_modify(|existing| {
                        if existing != canonical {
                            *existing = String::new();
                        }
                    })
                    .or_insert_with(|| canonical.to_string());
            }
            walk(sub, map);
        }
    }
    let mut map = BTreeMap::new();
    walk(cmd, &mut map);
    map.retain(|_, v| !v.is_empty());
    map
}

/// 把缩写候选行的描述替换为 `= <全称>`
fn annotate_alias(shell: Shell, line: &str, aliases: &BTreeMap<String, String>) -> String {
    let annotation = |canonical: &str| format!("= {canonical}");

    match shell {
        // 'ws:工作空间管理' \
        Shell::Zsh => {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix('\'')
                && let Some(colon) = rest.find(':')
                && let Some(canonical) = aliases.get(&rest[..colon])
                && let Some(last_quote) = line.rfind('\'')
            {
                let indent = &line[..line.len() - t.len()];
                return format!(
                    "{indent}'{}:{}'{}",
                    &rest[..colon],
                    annotation(canonical),
                    &line[last_quote + 1..]
                );
            }
            line.to_owned()
        }
        // ... -a "ws" -d '工作空间管理'
        Shell::Fish => {
            if let Some(start) = line.find("-a \"")
                && let Some(rest) = line[start + 4..].split('"').next()
                && let Some(canonical) = aliases.get(rest)
                && let Some(d) = line.find("-d '")
                && let Some(close) = line[d + 4..].rfind('\'')
            {
                let end = d + 4 + close;
                // line[..d+4] 含开引号，line[end..] 含闭引号及之后内容
                return format!(
                    "{}{}{}",
                    &line[..d + 4],
                    annotation(canonical),
                    &line[end..]
                );
            }
            line.to_owned()
        }
        // [CompletionResult]::new('ws', 'ws', ..., '工作空间管理')
        Shell::PowerShell => {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("[CompletionResult]::new('")
                && let Some(quote) = rest.find('\'')
                && let Some(canonical) = aliases.get(&rest[..quote])
                && let Some(last_quote) = line.rfind('\'')
                && let Some(prev_quote) = line[..last_quote].rfind('\'')
            {
                return format!(
                    "{}{}{}",
                    &line[..prev_quote + 1],
                    annotation(canonical),
                    &line[last_quote..]
                );
            }
            line.to_owned()
        }
        // cand ws '工作空间管理'
        Shell::Elvish => {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("cand ")
                && let Some(space) = rest.find(' ')
                && aliases.contains_key(&rest[..space])
                && let Some(open) = line.find('\'')
                && let Some(close) = line.rfind('\'')
                && open < close
            {
                let canonical = &aliases[&rest[..space]];
                return format!(
                    "{}{}{}",
                    &line[..open + 1],
                    annotation(canonical),
                    &line[close..]
                );
            }
            line.to_owned()
        }
        _ => line.to_owned(),
    }
}
