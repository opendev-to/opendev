"""LLM API call methods."""

from typing import Any, Optional


class LlmCallsMixin:
    """Mixin for LLM call methods."""

    @staticmethod
    def _clean_messages(messages: list[dict]) -> list[dict]:
        """Strip internal ``_``-prefixed keys from messages before API calls."""
        return [
            {k: v for k, v in msg.items() if not k.startswith("_")}
            if any(k.startswith("_") for k in msg)
            else msg
            for msg in messages
        ]

    def call_thinking_llm(
        self,
        messages: list[dict],
        task_monitor: Optional[Any] = None,
    ) -> dict:
        """Call LLM for thinking phase only - NO tools, just reasoning.

        This makes a separate LLM call using the thinking system prompt
        to get pure reasoning/analysis before the action phase.

        Args:
            messages: Conversation messages (will use thinking system prompt)
            task_monitor: Optional monitor for tracking progress

        Returns:
            Dict with success status and thinking content
        """
        # Use thinking model if configured, otherwise normal model
        if self.config.model_thinking:
            model_id = self.config.model_thinking
            http_client = self._thinking_http_client or self._http_client
        else:
            model_id = self.config.model
            http_client = self._http_client

        # NO tools - pure reasoning
        payload = {
            "model": model_id,
            "messages": self._clean_messages(messages),
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(model_id, self.config.max_tokens),
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "content": "",
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
                "content": "",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else ""

        return {
            "success": True,
            "content": cleaned_content,
        }

    def call_critique_llm(
        self,
        thinking_trace: str,
        task_monitor: Optional[Any] = None,
    ) -> dict:
        """Call LLM to critique a thinking trace.

        This makes a separate LLM call to analyze and critique the reasoning
        in a thinking trace, providing feedback to improve it.

        Args:
            thinking_trace: The thinking trace to critique
            task_monitor: Optional monitor for tracking progress

        Returns:
            Dict with success status and critique content
        """
        from opendev.core.agents.prompts import load_prompt

        # Use critique model if configured, fallback to thinking, then normal
        if self.config.model_critique:
            model_id = self.config.model_critique
            http_client = (
                self._critique_http_client or self._thinking_http_client or self._http_client
            )
        elif self.config.model_thinking:
            model_id = self.config.model_thinking
            http_client = self._thinking_http_client or self._http_client
        else:
            model_id = self.config.model
            http_client = self._http_client

        # Load critique system prompt
        critique_system_prompt = load_prompt("system/critique")

        # Build messages for critique
        critique_messages = [
            {"role": "system", "content": critique_system_prompt},
            {
                "role": "user",
                "content": f"Please critique the following thinking trace:\n\n{thinking_trace}",
            },
        ]

        # NO tools - pure critique
        payload = {
            "model": model_id,
            "messages": critique_messages,
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(
                model_id, min(2048, self.config.max_tokens)
            ),  # Limit critique length
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "content": "",
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
                "content": "",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else ""

        return {
            "success": True,
            "content": cleaned_content,
        }

    def call_llm(
        self,
        messages: list[dict],
        task_monitor: Optional[Any] = None,
        thinking_visible: bool = True,
    ) -> dict:
        """Call LLM with tools for action phase.

        Args:
            messages: Conversation messages
            task_monitor: Optional monitor for tracking progress
            thinking_visible: If False, excludes think tool from schemas

        Returns:
            Dict with success status, content, tool_calls, etc.
        """
        # Route to VLM model when images are present, otherwise use normal model
        model_id, http_client = self._resolve_vlm_model_and_client(messages)

        # Rebuild schemas with current thinking visibility
        # Think tool is excluded from schemas since thinking is now a pre-processing step
        tool_schemas = self._schema_builder.build(thinking_visible=False)

        # Always use auto tool_choice - no more force_think
        tool_choice = "auto"

        payload = {
            "model": model_id,
            "messages": self._clean_messages(messages),
            "tools": tool_schemas,
            "tool_choice": tool_choice,
            **http_client.build_temperature_param(model_id, self.config.temperature),
            **http_client.build_max_tokens_param(model_id, self.config.max_tokens),
        }

        result = http_client.post_json(payload, task_monitor=task_monitor)
        if not result.success or result.response is None:
            return {
                "success": False,
                "error": result.error or "Unknown error",
                "interrupted": result.interrupted,
            }

        response = result.response
        if response.status_code != 200:
            return {
                "success": False,
                "error": f"API Error {response.status_code}: {response.text}",
            }

        response_data = response.json()
        choice = response_data["choices"][0]
        message_data = choice["message"]

        raw_content = message_data.get("content")
        cleaned_content = self._response_cleaner.clean(raw_content) if raw_content else None

        # Extract reasoning_content for OpenAI reasoning models (o1, o3, etc.)
        # This is the native thinking/reasoning trace from models like o1-preview
        reasoning_content = message_data.get("reasoning_content")

        if task_monitor and "usage" in response_data:
            usage = response_data["usage"]
            total_tokens = usage.get("total_tokens", 0)
            if total_tokens > 0:
                task_monitor.update_tokens(total_tokens)

        return {
            "success": True,
            "message": message_data,
            "content": cleaned_content,
            "tool_calls": message_data.get("tool_calls"),
            "reasoning_content": reasoning_content,  # Native thinking trace from model
            "usage": response_data.get("usage"),
        }
