//! 命令层（clap 类型）到 domain/service 类型的转换辅助

use std::io::IsTerminal;

use crate::entity::workspace::Model as WsModel;

use crate::app::AppCtx;
use crate::cli::{SelectArgs, TagMutationArgs};
use crate::domain::tags::{TagSet, tokens_to_set};
use crate::error::{AppError, AppResult};
use crate::service::tags::TagMutation;
use crate::service::workspace::{self, Fallback, WsSelector};

pub(crate) fn tag_set(tokens: Vec<String>) -> AppResult<TagSet> {
    tokens_to_set(&tokens)
}

pub(crate) fn remove_specs(tokens: Vec<String>) -> AppResult<Vec<(String, Option<Vec<String>>)>> {
    tokens
        .iter()
        .map(|t| crate::domain::tags::parse_key_opt_values(t))
        .collect()
}

pub(crate) fn mutation(args: TagMutationArgs) -> AppResult<TagMutation> {
    Ok(TagMutation {
        add: tag_set(args.add)?,
        set: tag_set(args.set)?,
        remove: remove_specs(args.remove)?,
    })
}

/// 解析 workspace；歧义且处于 TTY 时弹出交互选择
pub(crate) async fn resolve_ws(
    ctx: &AppCtx,
    args: &SelectArgs,
    fallback: Fallback,
) -> AppResult<WsModel> {
    let selector = WsSelector {
        name: args.workspace.clone(),
        select: args.select.clone(),
    };
    match workspace::resolve(&ctx.db, selector, fallback).await {
        Ok(ws) => Ok(ws),
        Err(AppError::Ambiguous {
            message,
            candidates,
        }) => {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                let names: Vec<String> = candidates
                    .iter()
                    .map(|c| {
                        c.split_once('=')
                            .map(|(_, n)| n.to_owned())
                            .unwrap_or(c.clone())
                    })
                    .collect();
                println!("{message}");
                let picked = dialoguer::Select::new()
                    .items(&names)
                    .default(0)
                    .interact_opt()
                    .map_err(|e| AppError::invalid_args(e.to_string()))?;
                match picked {
                    Some(i) => {
                        workspace::resolve(
                            &ctx.db,
                            WsSelector {
                                name: Some(names[i].clone()),
                                select: vec![],
                            },
                            Fallback::Error,
                        )
                        .await
                    }
                    None => Err(AppError::invalid_args("已取消")),
                }
            } else {
                let hint = candidates
                    .iter()
                    .map(|c| format!("  {c}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                Err(AppError::ambiguous(
                    format!("{message}\n{hint}"),
                    candidates,
                ))
            }
        }
        Err(other) => Err(other),
    }
}
