use std::collections::HashMap;

use anyhow::{Context, bail};

use crate::rule::{CrawlRuleProp, CrawlRulePropKind};

pub struct EvaluatedCrawlRuleProps(HashMap<String, String>);

impl EvaluatedCrawlRuleProps {
    pub fn substitute(&self, haystack: &str) -> String {
        let mut haystack = haystack.to_owned();

        for (prop_name, prop_value) in &self.0 {
            let prop_needle = format!("{{{prop_name}}}");
            haystack = haystack.replace(&prop_needle, prop_value);
        }

        haystack
    }
}

pub fn evaluate_rule_props(props: &[CrawlRuleProp], tab: &headless_chrome::Tab) -> anyhow::Result<EvaluatedCrawlRuleProps> {
    let mut evaluated_props = HashMap::new();

    for prop in props {
        if evaluated_props.contains_key(&prop.name) {
            bail!("Multiple rule props with name '{}'", &prop.name);
        }

        let value = match &prop.kind {
            CrawlRulePropKind::Script { script } => {
                let root = tab.find_element(":root").context("Finding :root element")?;

                let script_return_value = root.call_js_fn(
                    &format!("function() {{ {script} }}"),
                    vec![],
                    false
                ).context("Running rule prop script")?;

                match script_return_value.value {
                    Some(serde_json::Value::String(value)) => value,
                    Some(serde_json::Value::Number(value)) => value.to_string(),
                    Some(serde_json::Value::Bool(value)) => value.to_string(),
                    Some(value) => bail!("Rule prop script returned '{}'. This is likely a mistake. Expecting a string, number or boolean", value),
                    None => bail!("Rule prop script didn't return a value"),
                }
            }
            CrawlRulePropKind::Attribute { attribute, selector, capture } => {
                let attribute_element = tab.find_element(selector)
                    .context("Finding attribute element using prop's selector")?;

                let attribute_value = attribute_element.get_attribute_value(attribute)?
                    .context("Prop's attribute doesn't exist")?;

                match capture {
                    Some(capture) => {
                        let captures = capture.captures(&attribute_value)
                            .context("Prop's attribute value capture regex didn't match")?;

                        // NOTE: Captures always includes the match for the entire regex, which we dont' care about. We want exactly one captured GROUP in addition to this.
                        if captures.len() != 2 {
                            bail!("Prop's attribute value capture regex captured {} groups, expected exactly one", captures.len());
                        }

                        let capture_match = captures.get(1).unwrap();
                        capture_match.as_str().to_owned()
                    }
                    None => attribute_value,
                }
            },
            CrawlRulePropKind::InnerTextElement { inner_text_element_selector } => {
                let inner_text_element = tab.find_element(inner_text_element_selector)
                    .context("Finding element using prop's selector")?;

                inner_text_element.get_inner_text()
                    .context("Getting element's inner text")?
            }
        };

        evaluated_props.insert(prop.name.to_owned(), value);
    }

    Ok(EvaluatedCrawlRuleProps(evaluated_props))
}
