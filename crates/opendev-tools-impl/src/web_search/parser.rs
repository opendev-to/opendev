//! HTML parsing utilities for DuckDuckGo search results.

/// A single search result.
#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct SearchResult {
    pub(super) title: String,
    pub(super) url: String,
    pub(super) snippet: String,
}

/// Parse DuckDuckGo HTML search results.
///
/// DuckDuckGo's HTML-only endpoint returns results inside
/// `<div class="result ...">` blocks. Each block contains:
/// - `<a class="result__a" href="...">title</a>`
/// - `<a class="result__snippet" ...>snippet</a>`
pub(super) fn parse_ddg_html(html: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();

    // Split by result blocks
    let parts: Vec<&str> = html.split("class=\"result__a\"").collect();

    for part in parts.iter().skip(1) {
        // Extract URL from href="..."
        let url = extract_attr(part, "href=\"")
            .map(|u| {
                // DuckDuckGo wraps URLs in redirect links
                if let Some(actual) = extract_redirect_url(u) {
                    actual
                } else {
                    u.to_string()
                }
            })
            .unwrap_or_default();

        // Extract title (text between > and </a>)
        let title = extract_tag_text(part).unwrap_or_default();

        // Extract snippet
        let snippet = if let Some(snippet_start) = part.find("result__snippet") {
            let snippet_part = &part[snippet_start..];
            extract_tag_text(snippet_part).unwrap_or_default()
        } else {
            String::new()
        };

        if !url.is_empty() && !title.is_empty() {
            results.push(SearchResult {
                title: strip_html_tags(&title),
                url,
                snippet: strip_html_tags(&snippet),
            });
        }
    }

    results
}

/// URL-encode a string for query parameters.
pub(super) fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                for b in encoded.bytes() {
                    result.push('%');
                    result.push_str(&format!("{b:02X}"));
                }
            }
        }
    }
    result
}

/// Extract the domain from a URL, stripping the `www.` prefix.
fn extract_domain(url: &str) -> Option<String> {
    // Simple domain extraction without pulling in the `url` crate.
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let domain = after_scheme.split('/').next().unwrap_or("");
    let domain = domain.split(':').next().unwrap_or(domain); // strip port
    let domain = domain.to_lowercase();
    let domain = domain.strip_prefix("www.").unwrap_or(&domain).to_string();
    if domain.is_empty() {
        None
    } else {
        Some(domain)
    }
}

/// Filter results by allowed/blocked domain lists.
pub(super) fn filter_by_domain(
    results: Vec<SearchResult>,
    allowed: &[String],
    blocked: &[String],
) -> Vec<SearchResult> {
    results
        .into_iter()
        .filter(|r| {
            let domain = match extract_domain(&r.url) {
                Some(d) => d,
                None => return false,
            };

            // Check allowed
            if !allowed.is_empty() {
                let passes = allowed.iter().any(|a| {
                    let clean = a.strip_prefix("www.").unwrap_or(a);
                    domain == clean || domain.ends_with(&format!(".{clean}"))
                });
                if !passes {
                    return false;
                }
            }

            // Check blocked
            if !blocked.is_empty() {
                let is_blocked = blocked.iter().any(|b| {
                    let clean = b.strip_prefix("www.").unwrap_or(b);
                    domain == clean || domain.ends_with(&format!(".{clean}"))
                });
                if is_blocked {
                    return false;
                }
            }

            true
        })
        .collect()
}

/// Extract an attribute value after the given prefix.
fn extract_attr<'a>(html: &'a str, prefix: &str) -> Option<&'a str> {
    let start = html.find(prefix)?;
    let rest = &html[start + prefix.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Extract text content after the first `>` until `</`.
fn extract_tag_text(html: &str) -> Option<String> {
    let start = html.find('>')? + 1;
    let rest = &html[start..];
    let end = rest.find("</").unwrap_or(rest.len().min(500));
    Some(html_decode(&rest[..end]).trim().to_string())
}

/// Extract the actual URL from DuckDuckGo's redirect URL.
///
/// DDG redirects look like: `//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&rut=...`
fn extract_redirect_url(url: &str) -> Option<String> {
    if url.contains("duckduckgo.com/l/") || url.contains("uddg=") {
        // Find uddg= parameter
        let uddg_start = url.find("uddg=")?;
        let rest = &url[uddg_start + 5..];
        let end = rest.find('&').unwrap_or(rest.len());
        let encoded = &rest[..end];
        Some(urldecode(encoded))
    } else if url.starts_with("//") {
        Some(format!("https:{url}"))
    } else {
        None
    }
}

/// Decode percent-encoded URL strings.
fn urldecode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next().unwrap_or(b'0');
            let lo = chars.next().unwrap_or(b'0');
            let val = hex_val(hi) * 16 + hex_val(lo);
            result.push(val as char);
        } else if b == b'+' {
            result.push(' ');
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

/// Decode common HTML entities.
fn html_decode(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

/// Strip HTML tags from a string, keeping only text content.
fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    // Collapse whitespace
    let collapsed: String = result.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_string()
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
