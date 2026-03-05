<!--
name: 'Tool Description: capture_screenshot'
description: Capture a screenshot of the screen
version: 2.0.0
-->

Capture a screenshot of the desktop screen and save it to a temporary location. Returns the file path to the saved screenshot.

## Usage notes

- Useful for debugging UI issues, verifying visual changes, or capturing the current state of a desktop application
- The screenshot is saved as a temporary file — reference the returned path to discuss or analyze it
- For capturing web pages specifically, use capture_web_screenshot instead (it handles full-page scrolling and dynamic content)
- To analyze the captured screenshot's content, use analyze_image with the returned file path
