<!--
name: 'Tool Description: open_browser'
description: Open a URL or local file in the browser
version: 2.0.0
-->

Open a URL or local file in the user's default web browser.

## Usage notes

- Useful for previewing web applications during development (e.g., "open http://localhost:3000"), viewing documentation, or opening HTML files
- Automatically handles localhost URLs and converts local file paths to file:// URLs
- This opens the page in the user's actual browser — it does not capture or return content. To fetch page content, use fetch_url instead
