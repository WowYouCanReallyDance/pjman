mod app;
mod cli;
mod commands;
mod config;
mod db;
mod domain;
mod entity;
mod error;
mod migration;
mod output;
mod service;
mod shell;

use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{app::AppCtx, cli::Cli, config::Config, error::AppResult};

#[tokio::main]
async fn main() {
    init_tracing();

    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> AppResult<()> {
    let cfg = Config::resolve(cli.home.clone(), cli.database_url.clone())?;
    let db = db::connect(&cfg).await?;
    let ctx = AppCtx {
        db,
        cfg,
        json: cli.json,
        yes: cli.yes,
    };
    commands::run(cli, ctx).await
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,sqlx=off"));
    // 日志必须走 stderr：stdout 承载 JSON、`jump --print` 路径等机器可读输出
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
