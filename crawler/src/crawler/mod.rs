use std::{path::{Path, PathBuf}, sync::Arc};

use anyhow::{Context, bail};
use tracing::{debug, info, error};
use url::Url;

use crate::{CLIArgs, crawler::{intercept::TabNetworkInterceptor, limiter::CrawlLimiter, subrule::CrawlSubruleContext}, index::{CrawlIndex, PageCrawlRow}, rule::CrawlRules};

pub mod subrule;
mod props;
mod limiter;
mod intercept;

pub struct Crawler {
    browser: headless_chrome::Browser,
    args: CLIArgs,
    // Wrapped in Arcs so that we can use these in blocking browser futures.
    rules: Arc<CrawlRules>,
    index: Arc<CrawlIndex>,
}

pub struct CrawledContent {
    pub output_path: PathBuf,
    pub data: Vec<u8>,
}

impl Crawler {
    pub async fn new(args: CLIArgs) -> anyhow::Result<Self> {
        let browser = match &args.chromium_service_url {
            Some(chromium_service_url) => browser_connect_external(chromium_service_url).await.context("Connecting to external Chromium")?,
            None => headless_chrome::Browser::default().context("Launching headless Chromium")?,
        };

        let rules = CrawlRules::load(&args.rules_path).await
            .context("Loading crawl rules")?;

        let index = CrawlIndex::load_or_create(&args.output_dir_path).await
            .context("Loading crawl index")?;

        Ok(Self {
            args,
            browser,
            rules: Arc::new(rules),
            index: Arc::new(index),
        })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let mut crawl_url = if let Some(crawl_url_override) = &self.args.crawl_url_override {
            crawl_url_override.to_owned()
        } else if let Some(next_pending_page_crawl_url) = self.take_next_pending_page_crawl_url().await? {
            next_pending_page_crawl_url
        } else if let Some(crawl_root_url) = &self.rules.crawl_root_url {
            Url::parse(&crawl_root_url)
                .context("Parsing crawl root URL")?
        } else {
            bail!("No root crawl URL specified nor pending crawls present. Set a root crawl URL, or specify a crawl URL explicitly using --crawl-url.");
        };

        let mut crawl_limiter = CrawlLimiter::new(self.rules.limiter.clone());

        loop {
            crawl_limiter.wait_and_hit().await;
            info!("Crawling '{}'...", crawl_url);
            self.crawl_page(crawl_url).await?;

            match self.take_next_pending_page_crawl_url().await? {
                Some(next_pending_page_crawl_url) => crawl_url = next_pending_page_crawl_url,
                None => break,
            }
        }

        Ok(())
    }

    async fn crawl_page(&self, crawl_url: Url) -> anyhow::Result<()> {
        // Browser is a cheap (Arc) wrapper.
        let browser = self.browser.clone();
        let rules = self.rules.clone();
        let index = self.index.clone();
        let output_dir_path = self.args.output_dir_path.clone();

        tokio_await_blocking(move || {
            let tab = browser_init_tab(&browser)?;
            let tab_network_interceptor = TabNetworkInterceptor::attach(&tab)?;
            let tab_url = browser_tab_navigate(&tab, crawl_url.as_str())?;

            let page_crawl_row = tokio_block_on(index.add_page_crawl(&tab_url, chrono::Utc::now()))?;

            for subrule in &rules.subrules {
                /*
                 * Preparation
                 */
                if !subrule.matches(&tab_url) {
                    debug!("Subrule didn't match, skipping.");
                    continue;
                }

                let subrule_props = match props::evaluate_rule_props(&subrule.props, &tab) {
                    Ok(props) => props,
                    Err(e) => {
                        error!(error = %e, subrule = &subrule.name, "Evaluating subrule props");
                        continue;
                    }
                };

                let crawl_subrule_ctx = CrawlSubruleContext {
                    browser: &browser,
                    tab: &tab,
                    subrule,
                    subrule_props: &subrule_props,
                    tab_network_interceptor: &tab_network_interceptor,
                };

                /*
                 * Crawl contents
                 */
                if let Err(e) = Self::crawl_page_with_subrule(crawl_subrule_ctx, &output_dir_path, index.clone(), &page_crawl_row) {
                    error!(error = %e, subrule = &subrule.name, "Crawling page contents with subrule");
                }

                /*
                 * Add followup crawl URLs
                 */
                for followup_crawl_url in &subrule.followup_crawl_urls {
                    let followup_crawl_url = subrule_props.substitute(followup_crawl_url);
                    tokio_block_on(index.add_pending_page_crawl(&followup_crawl_url, None))?;
                }
            }

            tab.close(true)?;

            Ok(())
        }).await
    }

    fn crawl_page_with_subrule(crawl_subrule_ctx: CrawlSubruleContext, output_dir_path: &Path, index: Arc<CrawlIndex>, page_crawl_row: &PageCrawlRow) -> anyhow::Result<()> {
        let subrule_crawled_content = subrule::subrule_crawl(crawl_subrule_ctx)?;

        /*
         * Save the crawled content
         */
        let content_full_output_path = output_dir_path.join(&subrule_crawled_content.output_path);

        if let Some(content_full_output_parent_path) = content_full_output_path.parent() {
            std::fs::create_dir_all(content_full_output_parent_path).context("Creating output directories")?;
        }
        std::fs::write(content_full_output_path, &subrule_crawled_content.data).context("Writing output")?;

        /*
         * Add the crawled content to the database
         */
        tokio_block_on(async {
            index.add_crawled_content(page_crawl_row, crawl_subrule_ctx.subrule, &subrule_crawled_content).await
        })?;

        info!(
            "Crawled content with rule '{}': {} bytes {}",
            crawl_subrule_ctx.subrule.name,
            subrule_crawled_content.data.len(),
            subrule_crawled_content.output_path.extension()
                .map(|os_str| os_str.to_string_lossy())
                .map(|ext| format!("({ext})"))
                .unwrap_or_default(),
        );

        Ok(())
    }

    async fn take_next_pending_page_crawl_url(&self) -> anyhow::Result<Option<Url>> {
        let maybe_pending_page_crawl_row = self.index.take_next_pending_page_crawl().await?;

        let maybe_pending_page_crawl_url = maybe_pending_page_crawl_row
            .map(|pending_page_crawl_row| Url::parse(&pending_page_crawl_row.page_url))
            .transpose()?;

        Ok(maybe_pending_page_crawl_url)
    }
}

async fn tokio_await_blocking<T>(blocking_task_fn: impl FnOnce() -> T + Send + 'static) -> T where T: Send + 'static {
    let task = tokio::task::spawn_blocking(move || blocking_task_fn());
    task.await.expect("Task panicked")
}

fn tokio_block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Handle::current().block_on(future)
}

fn browser_init_tab(browser: &headless_chrome::Browser) -> anyhow::Result<Arc<headless_chrome::Tab>> {
    let tab = browser.new_tab()?;
    tab.enable_stealth_mode()?;
    tab.activate()?;

    Ok(tab)
}

fn browser_tab_navigate(tab: &headless_chrome::Tab, url: &str) -> anyhow::Result<String> {
    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;

    let navigated_url = tab.get_url();
    Ok(navigated_url)
}

async fn browser_connect_external(chromium_service_url: &Url) -> anyhow::Result<headless_chrome::Browser> {
    /*
     * Get the Chromium instance's websocket debugger url using the /json/version endpoint exposed by remote debugging.
     * chromium_service_url should be HTTP to the port specified to Chromium using --remote-debugging-port.
     */
    let version_response = reqwest::get(chromium_service_url.join("/json/version").unwrap().as_str())
        .await
        .context("Fetching /json/version response")?;

    let version_response_bytes = version_response.bytes().await?;

    // Example response:
    // {
    //     "Browser": "Chrome/150.0.7871.114",
    //     "Protocol-Version": "1.3",
    //     "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36",
    //     "V8-Version": "15.0.245.15",
    //     "WebKit-Version": "537.36 (@f405107495a07cb1bfcf687d4af8d91117098db6)",
    //     "webSocketDebuggerUrl": "ws://127.0.0.1:9222/devtools/browser/f38615a4-35f4-48c6-bea1-cb3c67bc85a4"
    // }
    let version_response_json: serde_json::Value = serde_json::from_slice(&version_response_bytes)?;

    let websocket_debugger_url = version_response_json.get("webSocketDebuggerUrl")
        .context("Chromium /json/version response JSON has no webSocketDebuggerUrl at the root level")?
        .as_str()
        .context("Chromium /json/version response JSON's webSocketDebuggerUrl is not a string?")?;

    debug!("Got WebSocket debugger URL: '{websocket_debugger_url}'");

    /*
     * Now we have the websocket debugger URL, just need to let headless_chrome handle the rest.
     */
    headless_chrome::Browser::connect(websocket_debugger_url.to_owned())
        .context("Connecting with headless_chrome")
}
