"""Prompt construction for REPL input."""

from prompt_toolkit.formatted_text import FormattedText


class PromptBuilder:
    """Builds formatted prompt tokens for prompt_toolkit."""

    @staticmethod
    def build_tokens() -> FormattedText:
        """Construct the prompt tokens used by prompt_toolkit.

        Returns:
            FormattedText with prompt styling
        """
        return FormattedText([
            ("", "│ "),
            ("", "› "),
        ])
