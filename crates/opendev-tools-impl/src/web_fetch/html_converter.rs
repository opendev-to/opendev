//! HTML-to-markdown conversion utilities.
//!
//! Regex-based extraction rather than a full DOM parser. Handles the most
//! common HTML patterns: headings, paragraphs, links, lists, code blocks,
//! emphasis, and removes scripts/styles/navigation.

use regex::Regex;

/// Convert HTML content to clean markdown for LLM-friendly output.
pub(super) fn html_to_markdown(html: &str) -> String {
    let mut text = html.to_string();

    // Remove script, style, nav, footer, header tags and their content
    for tag in &[
        "script", "style", "nav", "footer", "header", "noscript", "svg",
    ] {
        if let Ok(re) = Regex::new(&format!(r"(?is)<{tag}[^>]*>.*?</{tag}>")) {
            text = re.replace_all(&text, "").to_string();
        }
    }

    // Remove HTML comments
    if let Ok(re) = Regex::new(r"(?s)<!--.*?-->") {
        text = re.replace_all(&text, "").to_string();
    }

    // Convert headings
    for level in 1..=6 {
        let prefix = "#".repeat(level);
        if let Ok(re) = Regex::new(&format!(r"(?i)<h{level}[^>]*>(.*?)</h{level}>")) {
            text = re
                .replace_all(&text, |caps: &regex::Captures| {
                    format!("\n\n{prefix} {}\n\n", strip_tags(&caps[1]))
                })
                .to_string();
        }
    }

    // Convert pre/code blocks
    if let Ok(re) = Regex::new(r"(?is)<pre[^>]*>\s*<code[^>]*>(.*?)</code>\s*</pre>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n\n```\n{}\n```\n\n", decode_entities(&caps[1]))
            })
            .to_string();
    }
    if let Ok(re) = Regex::new(r"(?is)<pre[^>]*>(.*?)</pre>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n\n```\n{}\n```\n\n", decode_entities(&caps[1]))
            })
            .to_string();
    }

    // Convert inline code
    if let Ok(re) = Regex::new(r"(?i)<code[^>]*>(.*?)</code>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("`{}`", decode_entities(&caps[1]))
            })
            .to_string();
    }

    // Convert links
    if let Ok(re) = Regex::new(r#"(?i)<a[^>]*href="([^"]*)"[^>]*>(.*?)</a>"#) {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let href = &caps[1];
                let link_text = strip_tags(&caps[2]);
                if link_text.is_empty() || href.starts_with('#') || href.starts_with("javascript:")
                {
                    link_text
                } else {
                    format!("[{link_text}]({href})")
                }
            })
            .to_string();
    }

    // Convert images
    if let Ok(re) = Regex::new(r#"(?i)<img[^>]*alt="([^"]*)"[^>]*src="([^"]*)"[^>]*/?>"#) {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("![{}]({})", &caps[1], &caps[2])
            })
            .to_string();
    }

    // Convert emphasis
    if let Ok(re) = Regex::new(r"(?i)<(?:strong|b)>(.*?)</(?:strong|b)>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("**{}**", strip_tags(&caps[1]))
            })
            .to_string();
    }
    if let Ok(re) = Regex::new(r"(?i)<(?:em|i)>(.*?)</(?:em|i)>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("*{}*", strip_tags(&caps[1]))
            })
            .to_string();
    }

    // Convert list items
    if let Ok(re) = Regex::new(r"(?i)<li[^>]*>(.*?)</li>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                format!("\n- {}", strip_tags(&caps[1]).trim())
            })
            .to_string();
    }

    // Convert blockquotes
    if let Ok(re) = Regex::new(r"(?is)<blockquote[^>]*>(.*?)</blockquote>") {
        text = re
            .replace_all(&text, |caps: &regex::Captures| {
                let content = strip_tags(&caps[1]);
                let quoted: Vec<String> = content.lines().map(|l| format!("> {l}")).collect();
                format!("\n\n{}\n\n", quoted.join("\n"))
            })
            .to_string();
    }

    // Convert <br> and <hr>
    if let Ok(re) = Regex::new(r"(?i)<br\s*/?>") {
        text = re.replace_all(&text, "\n").to_string();
    }
    if let Ok(re) = Regex::new(r"(?i)<hr\s*/?>") {
        text = re.replace_all(&text, "\n\n---\n\n").to_string();
    }

    // Convert paragraphs and divs to double newlines
    if let Ok(re) = Regex::new(r"(?i)</?(?:p|div|section|article|main)[^>]*>") {
        text = re.replace_all(&text, "\n\n").to_string();
    }

    // Remove remaining HTML tags
    text = strip_tags(&text);

    // Decode HTML entities
    text = decode_entities(&text);

    // Clean up whitespace: collapse multiple blank lines, trim lines
    if let Ok(re) = Regex::new(r"\n{3,}") {
        text = re.replace_all(&text, "\n\n").to_string();
    }
    // Collapse multiple spaces within lines
    if let Ok(re) = Regex::new(r"[ \t]{2,}") {
        text = re.replace_all(&text, " ").to_string();
    }

    text.trim().to_string()
}

/// Strip all HTML tags from text.
pub(super) fn strip_tags(html: &str) -> String {
    if let Ok(re) = Regex::new(r"<[^>]*>") {
        re.replace_all(html, "").to_string()
    } else {
        html.to_string()
    }
}

/// Decode common HTML entities.
pub(super) fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&copy;", "©")
        .replace("&reg;", "®")
        .replace("&trade;", "™")
}

#[cfg(test)]
#[path = "html_converter_tests.rs"]
mod tests;
