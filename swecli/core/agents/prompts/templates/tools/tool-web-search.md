<!--
name: 'Tool Description: web_search'
description: Search the web for current information
version: 2.0.0
-->

Search the web for current information using DuckDuckGo. Returns results with titles, URLs, and snippets.

## Usage notes

- Use this for finding up-to-date information, documentation, tutorials, recent events, and answers beyond your knowledge cutoff
- Results are formatted as markdown links for easy reference
- MANDATORY: After answering the user's question using web search results, you MUST include a "Sources:" section at the end of your response listing all relevant URLs as markdown hyperlinks:
  Sources:
  - [Source Title 1](https://example.com/1)
  - [Source Title 2](https://example.com/2)
- Use the current year in search queries when looking for recent information or documentation. Do not search with outdated years
- Domain filtering is supported — use include_domains or exclude_domains to narrow results
- For fetching and analyzing the full content of a specific page, use fetch_url after finding the URL with web_search
