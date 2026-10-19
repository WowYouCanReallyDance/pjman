use std::collections::BTreeMap;

use serde::Serialize;

use crate::app::AppCtx;
use crate::cli::TagsCmd;
use crate::commands::common::remove_specs;
use crate::domain::tags::tokens_to_set;
use crate::error::AppResult;
use crate::service::tags::dict_remove;
use crate::{output, service};

pub async fn run(cmd: TagsCmd, ctx: &AppCtx) -> AppResult<()> {
    match cmd {
        TagsCmd::Add { tags } => add(ctx, tags).await,
        TagsCmd::Remove { tags, force } => remove(ctx, tags, force).await,
        TagsCmd::List { key } => list(ctx, key.as_deref()).await,
    }
}

async fn add(ctx: &AppCtx, tokens: Vec<String>) -> AppResult<()> {
    let set = tokens_to_set(&tokens)?;
    let models = service::tags::ensure_tag_set(&ctx.db, &set).await?;
    let mut added: Vec<String> = models
        .into_iter()
        .map(|t| format!("{}={}", t.tag_key, t.tag_value))
        .collect();
    added.sort();
    added.dedup();
    if ctx.json {
        output::print_json(&added)?;
    } else {
        for tag in added {
            println!("已添加可用 tag：{tag}");
        }
    }
    Ok(())
}

async fn remove(ctx: &AppCtx, tokens: Vec<String>, force: bool) -> AppResult<()> {
    let specs = remove_specs(tokens)?;
    let removed = dict_remove(&ctx.db, specs, force).await?;
    if ctx.json {
        output::print_json(&removed)?;
    } else {
        for tag in removed {
            println!("已删除 tag：{tag}");
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct TagView {
    key: String,
    values: Vec<String>,
}

async fn list(ctx: &AppCtx, key: Option<&str>) -> AppResult<()> {
    let rows = service::tags::dict_list(&ctx.db, key).await?;

    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        grouped.entry(row.tag_key).or_default().push(row.tag_value);
    }

    if ctx.json {
        let views: Vec<TagView> = grouped
            .into_iter()
            .map(|(key, values)| TagView { key, values })
            .collect();
        output::print_json(&views)?;
        return Ok(());
    }

    let table_rows: Vec<Vec<String>> = grouped
        .into_iter()
        .map(|(key, values)| vec![key, values.join(",")])
        .collect();
    output::print_table(&["KEY", "VALUES"], table_rows);
    Ok(())
}
