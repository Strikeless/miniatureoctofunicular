# miniatureoctofunicular
A semi-modular website crawler, useful for automated crawling of individual snippets of content in concretely formatted webpages.

Though there are legitimate use cases such as getting backups of important content for peace of mind, there is naturally a lot of capability for abuse with a tool like this, which is why I ask you to:
- Do not use this for leeching profit or personal gain off of the work of others (Shame on you if you do!)
- Respect the host of whatever you're crawling by configuring appropriate rate limiting on your end. (Have decency towards smaller hosts or you'll force them to use countermeasures, making the web worse for all of us)

This program is licensed under version 3 of the GNU Affero General Public License. You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

## Target use case
This is not a general purpose "save as HTML" crawler, but rather something for storing specific snippets of text/media from pages with a common format in an organized manner.

I also wouldn't consider this feature-complete. The program probably lacks something you want for your oddly specific crawling needs,
but the nice thing is that it's probably faster for you to glue together any features you need and write a quick and dirty crawl configuration,
than to write an oddly specific crawler from scratch for your oddly specific use case, starting all over again when you want to crawl something else.

If you decide to extend the program, then PRs are appreciated and welcome, assuming **you** wrote something maintainable that could be useful to others. No code from LLMs please, especially if you would have a hard time without it.

## Chromium requirement
We're driving Chromium through the CDP protocol for all browsing needs, so you'll need a Chromium-based browser running with remote debugging capabilities.

You can launch Chromium with remote debugging using `chromium --remote-debugging-port=<PORT>` and then pass the port to the crawler using `--chromium-service-url "http://localhost:<PORT>"`.

## Example usage and configuration
As an example to show the main features, we'll be writing a configuration to store the text of random Wikipedia articles.

The configuration schema is expected to evolve significantly in future versions, but is already capable enough for many tasks.
```json5
{
    // This is the URL that crawling will start from whenever there are no pending crawl URLs in an existing session and no starting URL is explicitly specified with a CLI argument.
    crawl_root_url: "https://en.wikipedia.org/wiki/Special:Random",
    // Individual "rules" specify what to crawl, where to store it and what pages to follow up with.
    subrules: [
        {
            name: "article_content_text",
            // Props/properties are values evaluated from the page, that can be embedded to most values of a rule using a {prop_name} syntax.
            props: [
                {
                    name: "article_title",
                    // This prop will get it's value from the inner text of an element matching the given CSS selector.
                    inner_text_element_selector: ":is(.mw-page-title-main, #firstHeading)"
                },
                /*
                {
                    name: "article_title",
                    script: "document.querySelector(':is(.mw-page-title-main, #firstHeading)').innerHTML"
                },
                {
                    name: "example_attr_prop",
                    attribute: "attribute-name",
                    selector: "#element-with-attribute",
                    // Optionally, a regex capturing the wanted portion of the attribute value.
                    capture: "unwanted-prefix-(.*)"
                }
                */
            ],
            // A CSS selector pointing to the element whose content this rule will store. If no element is found with this selector, the rule is ignored.
            // Here we are selecting plain HTML, so the element's inner HTML will be stored. We could also select an image for example, where the actual media would be stored.
            // Properties are supported, so we could do something like ".{article_title}" if needed.
            selector: "#mw-content-text",
            // Here we are using article title property specified above. The selected content would be stored to "./articles/Example article.html" or such.
            // Extension will be added automatically if it can be inferred from the selected element or it's media, though you could explicitly specify it.
            output_path: "articles/{article_title}",
            // Here we can specify URLs for pages that will be crawled afterwards, if this rule applied.
            // In this example, we are simply crawling another random article from wikipedia, creating a loop.
            followup_crawl_urls: ["https://en.wikipedia.org/wiki/Special:Random"]
        },
        // Articles can have images, these are <img> elements with the mw-file-element class.
        // If the article being crawled has an image like this, then this rule will apply, and store the image media to "./articles/Example article/example_image.jpg" or such.
        // Note that we're only expecting one image per article here, and this won't work properly with multiple images;
        // This is an oversight that can't be handled properly and trivially with the current configuration schema.
        {
            name: "article_content_image",
            props: [
                // Props are specific to individual rules, so we need to repeat the article_title prop definition here as well.
                // This is an oversight in the configuration schema, and will probably be resolved by changes in the future.
                {
                    name: "article_title",
                    inner_text_element_selector: ":is(.mw-page-title-main, #firstHeading)"
                }
            ],
            selector: "img.mw-file-element",
            output_path: "articles/{article_title}/image",
            followup_crawl_urls: []
        }
    ],
    // Self-inflicted rate limiting, it's there so use it!
    "limiter": {
        page_interval_millis: 5000,
        page_interval_jitter_millis: 2000,
    }
}
```
