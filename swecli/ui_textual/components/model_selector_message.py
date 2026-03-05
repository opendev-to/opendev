"""Model selector message component for interactive model switching."""

from typing import List, Tuple
from swecli.config import get_model_registry


def create_model_selector_message(selected_index: int, selection_mode: str = "normal") -> str:
    """Create an interactive model selector message with arrow key navigation.

    Args:
        selected_index: Index of currently selected item (0-based)
        selection_mode: Which model slot to select for ("normal", "thinking", "vlm")

    Returns:
        Formatted message string with ANSI color codes
    """
    registry = get_model_registry()

    # Build list of all items (providers and their models)
    items: List[Tuple[str, str, str, str, str]] = []  # (type, provider_id, model_id, display_text, category)

    # Map selection mode to header text
    mode_headers = {
        "normal": "Normal Model (Standard Coding Tasks)",
        "thinking": "Thinking Model (Complex Reasoning)",
        "vlm": "Vision Model (Image Processing)"
    }

    # Map selection mode to required capabilities
    mode_capabilities = {
        "normal": ["text"],  # All models have text
        "thinking": ["reasoning"],
        "vlm": ["vision"]
    }

    required_caps = mode_capabilities.get(selection_mode, ["text"])

    # Add "Back to categories" option as first item
    items.append((
        "back",
        "",
        "",
        "← Back to category selection",
        selection_mode
    ))

    for provider_info in registry.list_providers():
        # Check if this is OpenAI or Anthropic (all models support all capabilities)
        is_universal_provider = provider_info.id in ["openai", "anthropic"]

        # Filter models by capability (unless universal provider)
        matching_models = []
        for model_info in provider_info.list_models():
            if is_universal_provider:
                # OpenAI/Anthropic: all models work for all modes
                matching_models.append(model_info)
            else:
                # Other providers: check capabilities
                has_required = any(cap in model_info.capabilities for cap in required_caps)
                if has_required or selection_mode == "normal":
                    # Normal mode shows all models as fallback
                    matching_models.append(model_info)

        if not matching_models:
            continue

        # Add provider header
        provider_suffix = " (All models support all tasks)" if is_universal_provider else ""
        items.append((
            "provider",
            provider_info.id,
            "",
            f"[{provider_info.name}] - {len(matching_models)} models{provider_suffix}",
            selection_mode
        ))

        # Add models under this provider
        for model_info in matching_models:
            # Format model info
            context_str = f"{model_info.context_length // 1000}k"

            # Show capabilities for non-universal providers
            caps_display = ""
            if not is_universal_provider:
                caps = ", ".join(model_info.capabilities)
                caps_display = f" [{caps}]"

            display = f"  {model_info.name} ({context_str}){caps_display}"

            items.append((
                "model",
                provider_info.id,
                model_info.id,
                display,
                selection_mode
            ))

    # ANSI color codes
    CYAN = "\033[36m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    MAGENTA = "\033[35m"
    DIM = "\033[2m"
    BOLD = "\033[1m"
    RESET = "\033[0m"
    REVERSE = "\033[7m"  # Inverse/highlight

    # Build the message with fixed width
    BOX_WIDTH = 75
    lines = []
    header_text = mode_headers.get(selection_mode, "Select Model")
    header_line = f"─ {header_text} "
    header_padding = BOX_WIDTH - len(header_line) - 2
    lines.append(f"{BOLD}{CYAN}╭{header_line}{'─' * header_padding}╮{RESET}")
    lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")
    lines.append(f"{CYAN}│{RESET} {BOLD}Use ↑/↓ arrows or j/k to navigate, Enter to select{RESET}{' ' * 24}│{RESET}")
    lines.append(f"{CYAN}│{RESET} {DIM}Press ESC or Ctrl+C to cancel{RESET}{' ' * 44}│{RESET}")
    lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")
    lines.append(f"{CYAN}├{'─' * BOX_WIDTH}┤{RESET}")

    # Add items with selection highlighting
    for idx, (item_type, provider_id, model_id, display_text, category) in enumerate(items):
        is_selected = (idx == selected_index)

        # Calculate padding to maintain box width
        content_len = len(display_text)
        if content_len > BOX_WIDTH - 3:
            # Truncate if too long
            display_text = display_text[:BOX_WIDTH - 6] + "..."
            content_len = BOX_WIDTH - 3
        padding = BOX_WIDTH - content_len - 2

        if item_type == "back":
            # Back button - show in magenta
            if is_selected:
                lines.append(f"{CYAN}│{RESET} {REVERSE}{MAGENTA}{display_text}{' ' * padding}{RESET}│{RESET}")
            else:
                lines.append(f"{CYAN}│{RESET} {MAGENTA}{display_text}{RESET}{' ' * padding}│{RESET}")
        elif item_type == "provider":
            # Provider header
            if is_selected:
                lines.append(f"{CYAN}│{RESET} {REVERSE}{YELLOW}{display_text}{' ' * padding}{RESET}│{RESET}")
            else:
                lines.append(f"{CYAN}│{RESET} {YELLOW}{display_text}{RESET}{' ' * padding}│{RESET}")
        else:
            # Model item
            if is_selected:
                lines.append(f"{CYAN}│{RESET} {REVERSE}{GREEN}{display_text}{' ' * padding}{RESET}│{RESET}")
            else:
                lines.append(f"{CYAN}│{RESET} {display_text}{' ' * padding}│{RESET}")

    lines.append(f"{BOLD}{CYAN}╰{'─' * BOX_WIDTH}╯{RESET}")

    return "\n".join(lines)


def get_model_items(selection_mode: str = "normal"):
    """Get list of selectable items (providers and models) filtered by capability.

    Args:
        selection_mode: Which model slot to select for ("normal", "thinking", "vlm")

    Returns:
        List of tuples: (type, provider_id, model_id, display_text, category)
            - type: "provider", "model", or "back"
            - provider_id: ID of the provider
            - model_id: ID of the model (empty for provider headers)
            - display_text: Text to display
            - category: selection_mode value
    """
    registry = get_model_registry()
    items = []

    # Add "Back to categories" option as first item
    items.append((
        "back",
        "",
        "",
        "← Back to category selection",
        selection_mode
    ))

    # Map selection mode to required capabilities
    mode_capabilities = {
        "normal": ["text"],
        "thinking": ["reasoning"],
        "vlm": ["vision"]
    }

    required_caps = mode_capabilities.get(selection_mode, ["text"])

    for provider_info in registry.list_providers():
        # Check if this is OpenAI or Anthropic (all models support all capabilities)
        is_universal_provider = provider_info.id in ["openai", "anthropic"]

        # Filter models by capability (unless universal provider)
        matching_models = []
        for model_info in provider_info.list_models():
            if is_universal_provider:
                matching_models.append(model_info)
            else:
                has_required = any(cap in model_info.capabilities for cap in required_caps)
                if has_required or selection_mode == "normal":
                    matching_models.append(model_info)

        if not matching_models:
            continue

        # Add provider header
        items.append((
            "provider",
            provider_info.id,
            "",
            f"[{provider_info.name}] - {len(matching_models)} models",
            selection_mode
        ))

        # Add models under this provider
        for model_info in matching_models:
            items.append((
                "model",
                provider_info.id,
                model_info.id,
                model_info.name,
                selection_mode
            ))

    return items
