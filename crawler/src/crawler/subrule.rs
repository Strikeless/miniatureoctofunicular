use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, bail};

use crate::{crawler::{CrawledContent, intercept::TabNetworkInterceptor, props::EvaluatedCrawlRuleProps}, rule::CrawlSubrule};

#[derive(Clone, Copy)]
pub struct CrawlSubruleContext<'a> {
    pub browser: &'a headless_chrome::Browser,
    pub tab: &'a headless_chrome::Tab,
    pub subrule: &'a CrawlSubrule,
    pub subrule_props: &'a EvaluatedCrawlRuleProps,
    pub tab_network_interceptor: &'a TabNetworkInterceptor,
}

pub fn subrule_crawl(ctx: CrawlSubruleContext) -> anyhow::Result<CrawledContent> {
    let crawl_element = {
        let substituted_selector = ctx.subrule_props.substitute(&ctx.subrule.selector);
        ctx.tab.find_element(&substituted_selector).context("Finding element with selector of subrule")?
    };

    let crawl_element_content = get_crawl_element_content(ctx, &crawl_element)?;
    Ok(crawl_element_content)
}

fn get_crawl_element_content(ctx: CrawlSubruleContext, element: &headless_chrome::Element<'_>) -> anyhow::Result<CrawledContent> {
    let (data, data_based_extension) = match element.tag_name.to_uppercase().as_str() {
        "VIDEO" => {
            // We really need streaming (IO.Read) response interception for this shit probably
            todo!()
        }
        // TODO: There's a fucking <picture> bruh
        "IMG" => {
            let mut img_sources = Vec::new();

            if let Some(src) = element.get_attribute_value("src")? {
                img_sources.push(src);
            }
            if let Some(srcset) = element.get_attribute_value("srcset")? {
                for srcset_entry in srcset.split(',') {
                    // See https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img#srcset
                    // I think this is correct? Honestly just skimmed through the documentation I just told you to see... Whoops.
                    let srcset_entry_url = srcset_entry
                        .trim()
                        .split_whitespace()
                        .next()
                        .unwrap();

                    img_sources.push(srcset_entry_url.to_owned());
                }
            }

            let mut img_source_urls = img_sources.into_iter();
            let (data, data_source_url) = loop {
                let Some(img_source_url) = img_source_urls.next() else {
                    bail!("No data for image");
                };

                // All of the URLs that TabNetworkInterceptor recognizes are absolute.
                // Images may have been specified with relative URLs, so we need to "canonicalize" those.
                let canonical_img_source_url = element.call_js_fn(
                    "function(url) { return new URL(url, document.baseURI).href; }",
                    [serde_json::Value::String(img_source_url)].to_vec(),
                    false
                )?;
                let canonical_img_source_url = canonical_img_source_url.value
                    .as_ref()
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned();

                if let Some(data) = ctx.tab_network_interceptor.take(&canonical_img_source_url) {
                    break (data, canonical_img_source_url);
                }
            };

            let data_source_url_ext = data_source_url
                .rsplit_once('.')
                .map(|(_l, r)| r.to_owned());

            (data, data_source_url_ext)
        }
        _ => {
            /*
             * Tag with no special treatment. Get it's inner HTML.
             */
            let element_inner_html_bytes = {
                let value_remote_object = element.call_js_fn(
                    "function() { return this.innerHTML; }",
                    vec![],
                    false,
                ).context("Getting element's inner HTML")?;

                let value = value_remote_object.value
                    .context("Getting element's inner HTML via JS returned no value?")?;

                let value_string = value
                    .as_str()
                    .context("Getting element's inner HTML via JS returned a non-string?")?;

                value_string
                    .as_bytes()
                    .to_owned()
            };

            (
                element_inner_html_bytes,
                Some("html".to_owned()),
            )
        }
    };

    let output_path = {
        let path_substituted_string = ctx.subrule_props.substitute(&ctx.subrule.output_path);
        let path = PathBuf::from_str(&path_substituted_string).context("Parsing subrule output path")?;

        if path.extension().is_some() {
            // Path already has an explicitly specified extension, use that.
            path
        } else if let Some(data_based_extension) = data_based_extension {
            // Path doesn't have an explicitly specified extension, but the crawled content implies an extension, so use that.
            path.with_extension(data_based_extension)
        } else {
            // No specified or content-implied extension. Roll on with no extension.
            path
        }
    };

    Ok(CrawledContent {
        output_path,
        data,
    })
}
