"""Category selector message component for choosing which model slot to configure."""

from typing import List, Tuple


def create_category_selector_message(selected_index: int, normal_configured: bool = False) -> str:
    """Create an interactive category selector for choosing model slot.

    Args:
        selected_index: Index of currently selected category (0-based)
        normal_configured: Whether the normal model has been configured

    Returns:
        Formatted message string with ANSI color codes
    """
    # Define categories with mandatory/optional indicators
    categories = [
        ("normal", "Normal Model [REQUIRED]", "Standard coding tasks - must be configured first"),
        ("thinking", "Thinking Model [Optional]", "Complex reasoning - falls back to Normal if not set"),
        ("critique", "Critique Model [Optional]", "Self-critique of reasoning - falls back to Thinking"),
        ("compact", "Compact Model [Optional]", "Context compaction - falls back to Normal if not set"),
        ("vlm", "Vision Model [Optional]", "Image processing - vision tasks unavailable if not set"),
        ("finish", "✓ Finish Configuration", "Save and exit model selection"),
    ]

    # ANSI color codes
    CYAN = "\033[36m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    MAGENTA = "\033[35m"
    DIM = "\033[2m"
    BOLD = "\033[1m"
    RESET = "\033[0m"
    REVERSE = "\033[7m"  # Inverse/highlight

    # Build the message - use fixed width for proper alignment
    BOX_WIDTH = 70  # Total characters inside the box borders
    lines = []

    # Add tip about Normal model requirement if not configured
    tip_text = ""
    if not normal_configured:
        tip_text = f"{YELLOW}⚠ Configure Normal Model first before optional models{RESET}"

    lines.append(f"{BOLD}{CYAN}╭─ Select Model Category ─────────────────────────────────╮{RESET}")
    lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")
    lines.append(f"{CYAN}│{RESET} {BOLD}Choose which model slot to configure:{RESET}{' ' * 32}│{RESET}")

    if tip_text:
        lines.append(f"{CYAN}│{RESET} {tip_text}{' ' * 16}│{RESET}")

    lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")
    lines.append(f"{CYAN}│{RESET} {DIM}Use ↑/↓ arrows or j/k, Enter to select{RESET}{' ' * 27}│{RESET}")
    lines.append(f"{CYAN}│{RESET} {DIM}Press ESC or Ctrl+C to cancel{RESET}{' ' * 39}│{RESET}")
    lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")
    lines.append(f"{CYAN}├{'─' * BOX_WIDTH}┤{RESET}")

    # Add category items
    for idx, (category_id, name, description) in enumerate(categories):
        is_selected = (idx == selected_index)
        is_disabled = not normal_configured and category_id not in ["normal", "finish"]
        is_finish = category_id == "finish"

        if is_disabled:
            # Disabled items (optional models when Normal not configured)
            name_line = f"  {name} [Configure Normal first]"
            desc_line = f"  {description}"
            name_padding = BOX_WIDTH - len(name_line) - 1
            desc_padding = BOX_WIDTH - len(desc_line) - 1
            lines.append(f"{CYAN}│{RESET} {DIM}{name_line}{RESET}{' ' * name_padding}│{RESET}")
            lines.append(f"{CYAN}│{RESET} {DIM}{desc_line}{RESET}{' ' * desc_padding}│{RESET}")
        elif is_finish:
            # Finish option - use magenta color to stand out
            if is_selected:
                name_line = f"▶ {name}"
                desc_line = f"  {description}"
                name_padding = BOX_WIDTH - len(name_line) - 1
                desc_padding = BOX_WIDTH - len(desc_line) - 1
                lines.append(f"{CYAN}│{RESET} {REVERSE}{MAGENTA}{name_line}{' ' * name_padding}{RESET}│{RESET}")
                lines.append(f"{CYAN}│{RESET} {REVERSE}{MAGENTA}{desc_line}{' ' * desc_padding}{RESET}│{RESET}")
            else:
                name_line = f"  {name}"
                desc_line = f"  {description}"
                name_padding = BOX_WIDTH - len(name_line) - 1
                desc_padding = BOX_WIDTH - len(desc_line) - 1
                lines.append(f"{CYAN}│{RESET} {MAGENTA}{name_line}{RESET}{' ' * name_padding}│{RESET}")
                lines.append(f"{CYAN}│{RESET} {DIM}{desc_line}{RESET}{' ' * desc_padding}│{RESET}")
        elif is_selected:
            # Selected items - pad to box width
            name_line = f"▶ {name}"
            desc_line = f"  {description}"
            name_padding = BOX_WIDTH - len(name_line) - 1
            desc_padding = BOX_WIDTH - len(desc_line) - 1
            lines.append(f"{CYAN}│{RESET} {REVERSE}{GREEN}{name_line}{' ' * name_padding}{RESET}│{RESET}")
            lines.append(f"{CYAN}│{RESET} {REVERSE}{GREEN}{desc_line}{' ' * desc_padding}{RESET}│{RESET}")
        else:
            # Non-selected items - pad to box width
            name_line = f"  {name}"
            desc_line = f"  {description}"
            name_padding = BOX_WIDTH - len(name_line) - 1
            desc_padding = BOX_WIDTH - len(desc_line) - 1
            lines.append(f"{CYAN}│{RESET} {BOLD}{name_line}{RESET}{' ' * name_padding}│{RESET}")
            lines.append(f"{CYAN}│{RESET} {DIM}{desc_line}{RESET}{' ' * desc_padding}│{RESET}")

        # Add spacing between items (except after last)
        if idx < len(categories) - 1:
            lines.append(f"{CYAN}│{' ' * BOX_WIDTH}│{RESET}")

    lines.append(f"{BOLD}{CYAN}╰{'─' * BOX_WIDTH}╯{RESET}")

    return "\n".join(lines)


def get_category_items(normal_configured: bool = False) -> List[Tuple[str, str, str, str, str, bool]]:
    """Get list of category items for selection.

    Args:
        normal_configured: Whether the normal model has been configured

    Returns:
        List of tuples: (type, category_id, "", display_text, category, is_disabled)
        Format matches model selector items for compatibility
    """
    categories = [
        ("normal", "Normal Model [REQUIRED]", "Standard coding tasks"),
        ("thinking", "Thinking Model [Optional]", "Complex reasoning tasks"),
        ("critique", "Critique Model [Optional]", "Self-critique of reasoning"),
        ("compact", "Compact Model [Optional]", "Context compaction summaries"),
        ("vlm", "Vision Model [Optional]", "Image processing tasks"),
        ("finish", "✓ Finish Configuration", "Save and exit model selection"),
    ]

    items = []
    for category_id, name, description in categories:
        is_disabled = not normal_configured and category_id not in ["normal", "finish"]
        items.append((
            "category",  # type
            category_id,  # category_id (used as provider_id slot)
            "",  # model_id (empty for categories)
            f"{name}: {description}",  # display_text
            category_id,  # category (used for filtering)
            is_disabled  # disabled flag
        ))

    return items
