<!--
name: 'System Prompt: OpenAI Provider Hints'
description: Provider-specific guidance for OpenAI models
version: 1.0.0
-->

# Provider-Specific Notes

- You are running on an OpenAI model via the OpenAI API
- Tool calls use the `function` calling convention with JSON arguments
- When reasoning models (o1, o3, o4-mini) are detected, temperature is not sent and the system prompt is sent as a developer message
- For vision tasks, images are passed as base64-encoded `image_url` content parts
- Structured output via `response_format` is available but not used by default
