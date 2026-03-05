<!--
name: 'Tool Description: fetch_url'
description: Fetch content from a URL or perform a deep crawl
version: 2.0.0
-->

Fetch content from a URL or perform a deep crawl across linked pages.

## Usage notes

- Automatically extracts text from HTML and converts to markdown for readability
- The URL must be a fully-formed valid URL. HTTP URLs will be automatically upgraded to HTTPS
- Content is capped at max_length (default 50,000 chars)
- Use deep_crawl=true with max_depth and max_pages to crawl documentation sites or multi-page content
- Includes a self-cleaning 15-minute cache — repeated fetches of the same URL return cached results
- When a URL redirects to a different host, the tool will inform you and provide the redirect URL. Make a new fetch_url request with the redirect URL
- IMPORTANT: This tool WILL FAIL for authenticated or private URLs (Google Docs, Confluence, Jira). If an MCP tool is available for authenticated access, use that instead
- IMPORTANT: Never generate or guess URLs. Only use URLs from user messages, local files, or web_search results
- For GitHub URLs, prefer using run_command with the gh CLI instead (e.g., gh pr view, gh issue view, gh api)
- For large pages, summarize the relevant content rather than including raw HTML or markdown in your response. Extract only the information needed to answer the user's question
- Limit quoted content to ~125 characters per excerpt for non-essential pages. Focus on the key information
