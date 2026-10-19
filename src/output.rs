use std::io::IsTerminal;

use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use serde::Serialize;

use crate::error::AppResult;

/// 打印 UTF-8 表格（非 TTY 时固定 100 列，避免宽度无限膨胀）
pub fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(|h| h.to_string()).collect::<Vec<_>>());
    if !std::io::stdout().is_terminal() {
        table.set_width(100);
    }
    for row in rows {
        table.add_row(row);
    }
    println!("{table}");
}

/// 打印 pretty JSON
pub fn print_json<T: Serialize>(value: &T) -> AppResult<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
