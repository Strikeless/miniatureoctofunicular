use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing::error;
use tracing_subscriber::EnvFilter;
use url::Url;

use crate::crawler::Crawler;

pub mod crawler;
pub mod rule;
pub mod index;

#[derive(Clone, Debug, clap::Parser)]
pub struct CLIArgs {
    #[clap(long = "chromium-service-url", help = "An URL to an external Chromium browser's remote debugging service. Start Chromium with --remote-debugging-port.")]
    pub chromium_service_url: Option<Url>,

    #[clap(long = "crawl-url", help = "An URL to start crawling from. This takes precedence over a crawl index and the crawl root URL of the rules.")]
    pub crawl_url_override: Option<Url>,

    #[clap(help = "Path to the crawl rules configuration file")]
    pub rules_path: PathBuf,

    #[clap(help = "Directory path to which crawled content and the crawl index are stored")]
    pub output_dir_path: PathBuf,

    #[clap(long = "log", default_value = "", help = "Logging configuration (tracing_subscriber env-filter format)")]
    pub log: String,
}

fn main() {
    let args = CLIArgs::parse();

    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env()
                .expect("Parsing tracing environment filters from RUST_LOG")
        )
        .with_env_filter(&args.log)
        .init();

    let run_result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build().expect("Building tokio runtime")
        .block_on(run(args));

    if let Err(e) = run_result {
        error!(error = ?e, "Fatal runtime error");
    }
}

async fn run(args: CLIArgs) -> anyhow::Result<()> {
    let crawler = Crawler::new(args).await.context("Initializing")?;
    crawler.run().await?;
    Ok(())
}
