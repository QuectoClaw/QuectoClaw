// QuectoClaw — Web search and fetch tools

use super::{Tool, ToolResult};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// Search providers
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, query: &str, count: usize) -> anyhow::Result<String>;
}

/// Brave Search API provider
pub struct BraveSearchProvider {
    api_key: String,
}

impl BraveSearchProvider {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl SearchProvider for BraveSearchProvider {
    async fn search(&self, query: &str, count: usize) -> anyhow::Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?;

        let body: Value = resp.json().await?;

        let mut result = String::new();
        if let Some(results) = body
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
        {
            for (i, item) in results.iter().enumerate() {
                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let desc = item
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                result.push_str(&format!("{}. {} - {}\n   {}\n\n", i + 1, title, url, desc));
            }
        }

        if result.is_empty() {
            result = "No results found.".into();
        }

        Ok(result)
    }
}

/// DuckDuckGo HTML scraper fallback
pub struct DuckDuckGoSearchProvider;

#[async_trait]
impl SearchProvider for DuckDuckGoSearchProvider {
    async fn search(&self, query: &str, count: usize) -> anyhow::Result<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(USER_AGENT)
            .build()?;

        let resp = client
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .send()
            .await?;

        let html = resp.text().await?;
        extract_ddg_results(&html, count)
    }
}

fn extract_ddg_results(html: &str, count: usize) -> anyhow::Result<String> {
    let title_re = Regex::new(r#"class="result__a"[^>]*>([^<]+)</a>"#)?;
    let url_re = Regex::new(r#"class="result__url"[^>]*>([^<]+)</a>"#)?;
    let snippet_re = Regex::new(r#"class="result__snippet"[^>]*>([^<]*(?:<[^>]*>[^<]*)*)</a>"#)?;

    let titles: Vec<&str> = title_re
        .captures_iter(html)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();
    let urls: Vec<&str> = url_re
        .captures_iter(html)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();
    let snippets: Vec<&str> = snippet_re
        .captures_iter(html)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();

    let mut result = String::new();
    for (i, title) in titles.iter().enumerate().take(count.min(titles.len())) {
        let url = urls.get(i).unwrap_or(&"");
        let snippet = snippets.get(i).unwrap_or(&"");
        let clean = strip_tags(snippet);
        result.push_str(&format!(
            "{}. {} - {}\n   {}\n\n",
            i + 1,
            title,
            url.trim(),
            clean
        ));
    }

    if result.is_empty() {
        result = "No results found.".into();
    }

    Ok(result)
}

fn strip_tags(html: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(html, "").to_string()
}

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

pub struct WebSearchTool {
    provider: Box<dyn SearchProvider>,
    max_results: usize,
}

impl WebSearchTool {
    pub fn new(brave_api_key: Option<String>, max_results: usize) -> Self {
        let provider: Box<dyn SearchProvider> =
            if let Some(key) = brave_api_key.filter(|k| !k.is_empty()) {
                Box::new(BraveSearchProvider::new(key))
            } else {
                Box::new(DuckDuckGoSearchProvider)
            };
        Self {
            provider,
            max_results,
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web for information"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "description": "Number of results (default: 5)" }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::error("query is required"),
        };
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(self.max_results);

        match self.provider.search(query, count).await {
            Ok(results) => ToolResult::success(results),
            Err(e) => ToolResult::error(format!("Search failed: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

pub struct WebFetchTool {
    max_chars: usize,
}

impl WebFetchTool {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    fn description(&self) -> &str {
        "Fetch the content of a web page and extract its text"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "max_chars": { "type": "integer", "description": "Maximum characters to return" }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::error("url is required"),
        };
        let max = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(self.max_chars);

        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent(USER_AGENT)
            .build()
        {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("Failed to create HTTP client: {}", e)),
        };

        let resp = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return ToolResult::error(format!("Failed to fetch URL: {}", e)),
        };

        let status = resp.status();
        if !status.is_success() {
            return ToolResult::error(format!("HTTP {}", status));
        }

        let body = match resp.text().await {
            Ok(t) => t,
            Err(e) => return ToolResult::error(format!("Failed to read response: {}", e)),
        };

        let mut text = extract_text(&body);

        if text.len() > max {
            text.truncate(max);
            text.push_str("\n... (truncated)");
        }

        if text.is_empty() {
            text = "(empty page)".into();
        }

        ToolResult::success(text)
    }
}

/// Simple HTML to text extraction (strips tags, collapse whitespace).
fn extract_text(html: &str) -> String {
    // Remove script and style elements
    let script_re = Regex::new(r"(?si)<script[^>]*>.*?</script>").unwrap();
    let style_re = Regex::new(r"(?si)<style[^>]*>.*?</style>").unwrap();
    let cleaned = script_re.replace_all(html, "");
    let cleaned = style_re.replace_all(&cleaned, "");

    // Strip all HTML tags
    let text = strip_tags(&cleaned);

    // Collapse whitespace
    let ws_re = Regex::new(r"\s+").unwrap();
    let text = ws_re.replace_all(&text, " ");

    // Decode basic HTML entities
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}
