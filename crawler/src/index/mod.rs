use std::{path::Path, sync::Arc};

use anyhow::Context;
use sqlx::{Sqlite, migrate, pool::PoolConnection, sqlite::{SqliteConnectOptions, SqlitePoolOptions}};
use tracing::debug;

use crate::{crawler::CrawledContent, rule::CrawlSubrule};

pub struct CrawlIndex(Arc<CrawlIndexInner>);

impl CrawlIndex {
    pub async fn load_or_create(crawl_directory_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::load_or_create_impl(crawl_directory_path.as_ref()).await
    }
    async fn load_or_create_impl(crawl_directory_path: &Path) -> anyhow::Result<Self> {
        let inner = {
            let db_pool = {
                let db_path = crawl_directory_path.join("_crawl_index.sqlite3");

                let connect_options = SqliteConnectOptions::new()
                    .filename(db_path)
                    .create_if_missing(true);

                let pool_options = SqlitePoolOptions::new();

                // SQLite won't create parent directories and fails if we don't do it manually.
                tokio::fs::create_dir_all(crawl_directory_path).await
                    .context("Creating parent directories for crawl index database")?;

                pool_options
                    .connect_with(connect_options)
                    .await
                    .context("Opening crawl index database")?
            };

            CrawlIndexInner {
                db_pool,
            }
        };

        // Run database migrations (including possible initialization)
        let mut db = inner.db_acquire().await?;
        const MIGRATIONS: sqlx::migrate::Migrator = migrate!("./src/index/migrations");
        debug!("{} crawler index database migrations are compiled in.", MIGRATIONS.iter().len());
        MIGRATIONS.run(&mut db).await.context("Running crawl index database migration(s)")?;

        Ok(Self(Arc::new(inner)))
    }

    pub async fn take_next_pending_page_crawl(&self) -> anyhow::Result<Option<PendingPageCrawlRow>> {
        const QUERY: &'static str = r#"
            DELETE FROM
                pending_page_crawl
            WHERE
                effective_start_timestamp_unix_secs IS NULL
                OR effective_start_timestamp_unix_secs >= UNIXEPOCH()
            -- These would require SQLite to be built with SQLITE_ENABLE_UPDATE_DELETE_LIMIT. SQLx doesn't appear to provide a way for us to do that, so for now, TODO.
            --ORDER BY
            --    effective_start_timestamp_unix_secs ASC NULLS LAST
            --LIMIT 1
            RETURNING *
        "#;

        let mut db = self.0.db_acquire().await?;
        let maybe_row = sqlx::query_as(QUERY)
            .fetch_optional(&mut *db)
            .await
            .context("Querying for take_next_pending_page_crawl")?;
        Ok(maybe_row)
    }

    pub async fn add_pending_page_crawl(&self, page_url: &str, effective_start: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<PendingPageCrawlRow> {
        let effective_start_timestamp_unix_secs = effective_start.map(|dt| dt.timestamp());

        const QUERY: &'static str = r#"
            INSERT INTO pending_page_crawl (page_url, effective_start_timestamp_unix_secs) VALUES
                ($1, $2)
                RETURNING *
        "#;

        let mut db = self.0.db_acquire().await?;
        let added_row = sqlx::query_as(QUERY)
            .bind(page_url)
            .bind(effective_start_timestamp_unix_secs)
            .fetch_one(&mut *db)
            .await
            .context("Querying for add_pending_page_crawl")?;

        Ok(added_row)
    }

    pub async fn add_page_crawl(&self, page_url: &str, crawled_dt: chrono::DateTime<chrono::Utc>) -> anyhow::Result<PageCrawlRow> {
        let crawled_timestamp_unix_secs = crawled_dt.timestamp();

        const QUERY: &'static str = r#"
            INSERT INTO page_crawl (page_url, timestamp_unix_secs) VALUES
                ($1, $2)
                RETURNING *
        "#;

        let mut db = self.0.db_acquire().await?;
        let added_row = sqlx::query_as(QUERY)
            .bind(page_url)
            .bind(crawled_timestamp_unix_secs)
            .fetch_one(&mut *db)
            .await
            .context("Querying for add_page_crawl")?;

        Ok(added_row)
    }

    pub async fn add_crawled_content(&self, page_crawl: &PageCrawlRow, crawler_subrule: &CrawlSubrule, crawled_content: &CrawledContent) -> anyhow::Result<CrawledContentRow> {
        let crawled_content_data_size_bytes = crawled_content.data.len();
        let crawled_content_data_checksum_blake3 = blake3::hash(&crawled_content.data);

        const QUERY: &'static str = r#"
            INSERT INTO crawled_content (page_crawl_id, crawler_subrule_name, output_path, output_size_bytes, output_checksum_blake3) VALUES
                ($1, $2, $3, $4, $5)
                RETURNING *
        "#;

        let mut db = self.0.db_acquire().await?;
        let added_row = sqlx::query_as(QUERY)
            .bind(page_crawl.id)
            .bind(&crawler_subrule.name)
            .bind(crawled_content.output_path.to_string_lossy())
            .bind(crawled_content_data_size_bytes as i64)
            .bind(crawled_content_data_checksum_blake3.as_slice())
            .fetch_one(&mut *db)
            .await
            .context("Querying for add_crawled_content")?;

        Ok(added_row)
    }
}

struct CrawlIndexInner {
    db_pool: sqlx::SqlitePool,
}

impl CrawlIndexInner {
    pub async fn db_acquire(&self) -> anyhow::Result<PoolConnection<Sqlite>> {
        self.db_pool.acquire().await
            .context("Acquiring crawl index database connection")
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct PageCrawlRow {
    pub id: i32,
    pub timestamp_unix_secs: i32,
    pub page_url: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct PendingPageCrawlRow {
    pub id: i32,
    pub effective_start_timestamp_unix_secs: i32,
    pub page_url: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CrawledContentRow {
    pub id: i32,
    pub page_crawl_id: i32,
    pub output_path: String,
    pub output_size_bytes: i64,
    pub output_checksum_blake3: Vec<u8>,
}
