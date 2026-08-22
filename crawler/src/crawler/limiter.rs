use std::time::Duration;

use crate::rule::CrawlLimiterRules;

pub struct CrawlLimiter {
    next_hit_instant: tokio::time::Instant,
    rules: CrawlLimiterRules,
}

impl CrawlLimiter {
    pub fn new(rules: CrawlLimiterRules) -> Self {
        Self {
            next_hit_instant: tokio::time::Instant::now(),
            rules,
        }
    }

    pub async fn wait(&self) {
        tokio::time::sleep_until(self.next_hit_instant).await;
    }

    pub async fn wait_and_hit(&mut self) {
        self.wait().await;

        let next_page_interval_millis = {
            let selected_jitter_millis = self.rules.page_interval_jitter_millis
                .map(|jitter| jitter as i32)
                .map(|jitter| rand::random_range(-jitter ..= jitter))
                .unwrap_or(0);

            let mut interval_millis = (self.rules.page_interval_millis as i32 + selected_jitter_millis) as u64;

            if let Some(interval_min_millis) = self.rules.page_interval_min_millis {
                interval_millis = interval_millis.max(interval_min_millis as u64);
            }
            if let Some(interval_min_millis) = self.rules.page_interval_min_millis {
                interval_millis = interval_millis.max(interval_min_millis as u64);
            }

            interval_millis
        };

        self.next_hit_instant = tokio::time::Instant::now() + Duration::from_millis(next_page_interval_millis);
    }
}
