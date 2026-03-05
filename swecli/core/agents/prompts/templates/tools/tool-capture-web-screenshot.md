<!--
name: 'Tool Description: capture_web_screenshot'
description: Capture a full-page screenshot of a web page
version: 2.0.0
-->

Capture a full-page screenshot (and optionally PDF) of a web page using a headless browser (Playwright via Crawl4AI).

## Usage notes

- Handles dynamic content — waits for page load, JavaScript rendering, and lazy-loaded elements
- Captures the full scrollable page, not just the visible viewport
- More robust than basic screenshot tools for complex web pages with dynamic content
- When to use vs capture_screenshot: use capture_web_screenshot for web pages and web applications; use capture_screenshot for desktop screen capture
- Use this when the user wants to screenshot a website, verify web UI changes, or capture a web application's current state
