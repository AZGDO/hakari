use super::{ToolResult, ToolResultMetadata};
use std::time::Instant;

const SEARCH_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
const USER_AGENT: &str = "hakari/0.1.0";

pub async fn execute_web_search(query: &str, max_results: Option<usize>) -> ToolResult {
    let query = query.trim();
    if query.is_empty() {
        return ToolResult {
            success: false,
            output: "Error: query cannot be empty".to_string(),
            metadata: ToolResultMetadata::default(),
        };
    }

    let limit = max_results.unwrap_or(5).clamp(1, 10);
    let started = Instant::now();

    let client = reqwest::Client::new();
    let response = match client
        .get(SEARCH_ENDPOINT)
        .query(&[("q", query)])
        .header("User-Agent", USER_AGENT)
        .header("Accept", "text/html,application/xhtml+xml")
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return ToolResult {
                success: false,
                output: format!("Web search request failed: {}", error),
                metadata: ToolResultMetadata::default(),
            };
        }
    };

    if !response.status().is_success() {
        return ToolResult {
            success: false,
            output: format!("Web search failed with status {}", response.status()),
            metadata: ToolResultMetadata {
                exit_code: Some(response.status().as_u16() as i32),
                execution_time_ms: Some(started.elapsed().as_millis() as u64),
                ..Default::default()
            },
        };
    }

    let html = match response.text().await {
        Ok(html) => html,
        Err(error) => {
            return ToolResult {
                success: false,
                output: format!("Failed to read web search response: {}", error),
                metadata: ToolResultMetadata::default(),
            };
        }
    };

    let results = parse_results(&html, limit);
    let elapsed = started.elapsed().as_millis() as u64;

    if results.is_empty() {
        return ToolResult {
            success: true,
            output: format!(
                "No web results found for `{}`. Try a more specific or broader query.",
                query
            ),
            metadata: ToolResultMetadata {
                execution_time_ms: Some(elapsed),
                ..Default::default()
            },
        };
    }

    let mut output = format!("Web search results for `{}`:\n\n", query);
    for (index, result) in results.iter().enumerate() {
        output.push_str(&format!("{}. {}\n", index + 1, result.title));
        output.push_str(&format!("   URL: {}\n", result.url));
        if !result.snippet.is_empty() {
            output.push_str(&format!("   {}\n", result.snippet));
        }
        output.push('\n');
    }

    ToolResult {
        success: true,
        output,
        metadata: ToolResultMetadata {
            execution_time_ms: Some(elapsed),
            ..Default::default()
        },
    }
}

#[derive(Debug)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn parse_results(html: &str, limit: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut search_from = 0usize;

    while results.len() < limit {
        let Some(anchor_start_rel) = html[search_from..].find("result__a") else {
            break;
        };
        let anchor_start = search_from + anchor_start_rel;
        let href_search = &html[anchor_start..];
        let Some(href_rel) = href_search.find("href=\"") else {
            search_from = anchor_start + 8;
            continue;
        };
        let href_start = anchor_start + href_rel + 6;
        let Some(href_end_rel) = html[href_start..].find('"') else {
            break;
        };
        let href_end = href_start + href_end_rel;
        let raw_url = &html[href_start..href_end];

        let text_start = match html[href_end..].find('>') {
            Some(rel) => href_end + rel + 1,
            None => break,
        };
        let text_end = match html[text_start..].find("</a>") {
            Some(rel) => text_start + rel,
            None => break,
        };
        let title = decode_html_entities(strip_tags(&html[text_start..text_end]).trim());

        let snippet = extract_snippet(html, text_end).unwrap_or_default();
        let url = decode_html_entities(raw_url);

        if !title.is_empty() && !url.is_empty() {
            results.push(SearchResult {
                title,
                url,
                snippet: decode_html_entities(snippet.trim()),
            });
        }

        search_from = text_end;
    }

    results
}

fn extract_snippet(html: &str, from: usize) -> Option<String> {
    let snippet_classes = ["result__snippet", "result__extras__url"];
    for class in snippet_classes {
        let marker = format!("class=\"{}\"", class);
        let rel = html[from..].find(&marker)?;
        let start = from + rel;
        let open_end = html[start..].find('>')? + start + 1;
        let close = html[open_end..].find("</")? + open_end;
        let text = strip_tags(&html[open_end..close]);
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}

fn strip_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#x2F;", "/")
}
