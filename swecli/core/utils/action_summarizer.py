"""Action summarizer for creating concise spinner text from LLM responses."""

from typing import Optional


class ActionSummarizer:
    """Summarize LLM responses into concise action descriptions for spinners."""

    def __init__(self, api_key: Optional[str] = None):
        """Initialize the action summarizer.

        Args:
            api_key: Anthropic API key (optional, only needed for LLM-based summarization)
        """
        self.api_key = api_key
        self._client = None

    @property
    def client(self):
        """Lazy-load the Anthropic client only when needed."""
        if self._client is None:
            try:
                import anthropic
                self._client = anthropic.Anthropic(api_key=self.api_key)
            except ImportError:
                raise ImportError("anthropic package required for LLM-based summarization")
        return self._client

    def summarize(self, llm_response: str, max_length: int = 60) -> str:
        """Summarize LLM response into a concise action description.

        Args:
            llm_response: The full LLM response to summarize
            max_length: Maximum length of the summary

        Returns:
            Concise action description suitable for spinner display
        """
        # Use Claude 3.5 Haiku for fast, cheap summarization
        try:
            response = self.client.messages.create(
                model="claude-3-5-haiku-20241022",
                max_tokens=50,
                temperature=0,
                messages=[{
                    "role": "user",
                    "content": f"""Extract the main action from this text and rephrase it as a concise present-tense action phrase (max {max_length} chars).

Examples:
- Input: "I'll search through the configuration files to find the mode toggle implementation"
  Output: "Searching configuration files for mode toggle"

- Input: "Let me read the file to understand the current implementation"
  Output: "Reading file to understand implementation"

- Input: "I need to analyze the code structure and identify the relevant components"
  Output: "Analyzing code structure"

Text to summarize:
{llm_response[:500]}

Return ONLY the concise action phrase, nothing else."""
                }]
            )

            summary = response.content[0].text.strip()

            # Remove quotes if present
            summary = summary.strip('"\'')

            # Truncate if too long
            if len(summary) > max_length:
                summary = summary[:max_length - 3] + "..."

            return summary

        except Exception as e:
            # Fallback: extract first sentence and truncate
            first_sentence = llm_response.split('.')[0].strip()
            if len(first_sentence) > max_length:
                return first_sentence[:max_length - 3] + "..."
            return first_sentence

    def summarize_fast(self, llm_response: str, max_length: int = 60) -> str:
        """Fast local summarization without API call (fallback).

        Args:
            llm_response: The full LLM response to summarize
            max_length: Maximum length of the summary

        Returns:
            Concise action description
        """
        # Simple heuristic-based extraction
        text = llm_response.strip()

        # Common action patterns
        action_patterns = [
            ("I'll ", ""),
            ("I will ", ""),
            ("Let me ", ""),
            ("I'm going to ", ""),
            ("I need to ", ""),
            ("First, I'll ", ""),
            ("Now I'll ", ""),
        ]

        # Remove common prefixes
        for prefix, replacement in action_patterns:
            if text.startswith(prefix):
                text = replacement + text[len(prefix):]
                break

        # Convert to present continuous tense
        text = self._to_present_continuous(text)

        # Take first clause
        for delimiter in [',', ' and ', ' then ']:
            if delimiter in text:
                text = text.split(delimiter)[0]
                break

        # Truncate if needed
        if len(text) > max_length:
            text = text[:max_length - 3] + "..."

        # Capitalize first letter
        if text:
            text = text[0].upper() + text[1:]

        return text

    def _to_present_continuous(self, text: str) -> str:
        """Convert action to present continuous tense.

        Args:
            text: Action text

        Returns:
            Text in present continuous tense
        """
        conversions = {
            "search": "Searching",
            "read": "Reading",
            "write": "Writing",
            "edit": "Editing",
            "analyze": "Analyzing",
            "check": "Checking",
            "update": "Updating",
            "create": "Creating",
            "delete": "Deleting",
            "find": "Finding",
            "look": "Looking",
            "examine": "Examining",
            "review": "Reviewing",
            "modify": "Modifying",
            "fix": "Fixing",
            "implement": "Implementing",
        }

        lower_text = text.lower()
        for verb, continuous in conversions.items():
            if lower_text.startswith(verb):
                return continuous + text[len(verb):]

        return text
