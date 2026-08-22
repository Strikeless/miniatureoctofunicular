use std::path::Path;

use anyhow::Context;
use regex::Regex;
use serde_with::serde_as;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrawlRules {
    /// The URL from which to start crawling, if no crawl index exists yet.
    /// If left unspecified, either an existing crawl index or manual specification with the "--crawl-url" CLI argument is required.
    pub crawl_root_url: Option<String>,
    pub subrules: Vec<CrawlSubrule>,
    pub limiter: CrawlLimiterRules,
}

impl CrawlRules {
    pub async fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let data = tokio::fs::read(path).await.context("Reading crawl rules configuration file")?;
        Self::deserialize(&data)
    }

    pub async fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let data = self.serialize();
        tokio::fs::write(path, data).await.context("Saving crawl rules configuration file")
    }

    pub fn deserialize(serialized_bytes: &[u8]) -> anyhow::Result<Self> {
        let serialized_string = String::from_utf8(serialized_bytes.to_owned())
            .context("Expected valid UTF-8")?;

        json5::from_str(&serialized_string)
            .context("Deserializing from JSON5")
    }

    pub fn serialize(&self) -> Vec<u8> {
        json5::to_string(self)
            .expect("Serializing as JSON5")
            .into_bytes()
    }
}

#[serde_as] // NOTE: Order matters, must be before serde derives!
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrawlSubrule {
    /// A name used to identify this subrule.
    pub name: String,
    /// A regex used to match URL's of webpages where this rule should be applicable.
    #[serde_as(as = "Option<serde_with::DisplayFromStr>")]
    pub url: Option<Regex>,
    /// Evaluated values that can be embedded in some values of the subrule using the `{prop_name}` syntax.
    pub props: Vec<CrawlRuleProp>,
    /// A CSS selector pointing to the element displaying the content that should be crawled. Properties are supported.
    /// If the element is a supported multimedia container (`<img>`, `<video>`), the multimedia content it presents will be crawled;
    /// Otherwise the inner HTML of the element will be crawled.
    pub selector: String,
    /// A file path relative to the crawl output directory, to which crawled content is saved. Properties are supported.
    /// If the file extension is omitted, we will try to append a sensible extension automatically (`.html`, or an extension in the download URL of media).
    pub output_path: String,
    /// A list of URLs for pages to crawl afterwards. Properties are supported.
    pub followup_crawl_urls: Vec<String>,
}
impl CrawlSubrule {
    pub fn matches(&self, url: &str) -> bool {
        self.url
            .as_ref()
            .is_none_or(|url_regex| url_regex.is_match(url))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrawlRuleProp {
    pub name: String,
    #[serde(flatten)]
    pub kind: CrawlRulePropKind,
}

#[serde_as] // NOTE: Order matters, must be before serde derives!
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum CrawlRulePropKind {
    Script {
        /// A JavaScript function body returning a string, number or boolean, to be used as the property's value. 
        script: String,
    },
    Attribute {
        /// Name of a HTML attribute, whose value will be used as the property's value.
        attribute: String,
        /// A CSS selector pointing to the element with the attribute.
        selector: String,
        /// A regex capturing the wanted portion of the attribute value to the first capture group.
        /// If unspecified, the entire value of the attribute will be used.
        #[serde_as(as = "Option<serde_with::DisplayFromStr>")]
        capture: Option<Regex>,
    },
    InnerTextElement {
        inner_text_element_selector: String,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrawlLimiterRules {
    pub page_interval_millis: u32,
    pub page_interval_jitter_millis: Option<u32>,
    pub page_interval_min_millis: Option<u32>,
    pub page_interval_max_millis: Option<u32>,
}
