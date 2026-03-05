<!--
name: 'Tool Description: analyze_image'
description: Analyze an image using a Vision Language Model
version: 2.0.0
-->

Analyze an image using the configured Vision Language Model (VLM). Use this when the user asks to analyze, describe, or extract information from images.

## Usage notes

- Supports both local image files (provide file path) and online URLs (provide URL)
- Only available if the user has configured a VLM model via the /models command. If not configured, inform the user they need to set one up
- When to use vs read_file for images: use analyze_image when you need intelligent analysis, descriptions, or information extraction from images; use read_file when you simply need to view/display an image
- Provide a clear, specific prompt describing what information you want to extract from the image
