"""Railway/clack-style rendering primitives for the setup wizard."""

import getpass
from typing import Optional

from rich.console import Console

from swecli.ui_textual.style_tokens import (
    ACCENT,
    ERROR,
    SUCCESS,
    WARNING,
    RAIL_BAR,
    RAIL_BOX_BR,
    RAIL_BOX_TR,
    RAIL_DASH,
    RAIL_END,
    RAIL_START,
    RAIL_STEP,
    RAIL_TEE,
)

console = Console()


def rail_intro(title: str, lines: list[str]) -> None:
    """Render the opening block of the wizard."""
    console.print(f"\n  [{ACCENT}]{RAIL_START}[/{ACCENT}]  [bold]{title}[/bold]")
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")
    for line in lines:
        console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  {line}")
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")


def rail_outro(message: str) -> None:
    """Render the closing block of the wizard."""
    console.print(f"  [{ACCENT}]{RAIL_END}[/{ACCENT}]  [bold]{message}[/bold]")
    console.print()


def rail_step(title: str, step_label: Optional[str] = None) -> None:
    """Render a step heading with a diamond marker."""
    label = f" {RAIL_DASH}{RAIL_DASH} Step {step_label}" if step_label else ""
    console.print(f"\n  [{ACCENT}]{RAIL_STEP}[/{ACCENT}]  [bold]{title}[/bold]{label}")


def rail_info_box(title: str, lines: list[str], step_label: Optional[str] = None) -> None:
    """Render an info box attached to the rail.

    All lines share the same total visual width W so borders align:
      Top:     ``  ◇  TITLE {dashes}╮``   → 2+1+2 + header + 1 + dashes + 1 = W
      Empty:   ``  │{spaces} │``           → 2+1 + spaces + 1+1 = W
      Content: ``  │  text{pad} │``        → 2+1+2 + text + pad + 1+1 = W
      Bottom:  ``  ├{dashes}╯``            → 2+1 + dashes + 1 = W
    """
    label = f" {RAIL_DASH}{RAIL_DASH} Step {step_label}" if step_label else ""
    header_text = f"{title}{label}"

    max_line_len = max((len(line) for line in lines), default=0)
    W = max(len(header_text) + 8, max_line_len + 8, 52)
    W = min(W, console.width - 4)

    # Top: ◇  Title ──────╮
    top_dashes = RAIL_DASH * max(W - len(header_text) - 7, 1)
    console.print(
        f"\n  [{ACCENT}]{RAIL_STEP}[/{ACCENT}]  [bold]{header_text}[/bold] "
        f"[{ACCENT}]{top_dashes}{RAIL_BOX_TR}[/{ACCENT}]"
    )

    # Empty line
    inner = " " * (W - 5)
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]{inner} [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")

    # Content lines
    for line in lines:
        pad = " " * max(W - len(line) - 7, 1)
        console.print(
            f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  {line}{pad} [{ACCENT}]{RAIL_BAR}[/{ACCENT}]"
        )

    # Empty line
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]{inner} [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")

    # Bottom: ├──────────╯
    bottom_dashes = RAIL_DASH * (W - 4)
    console.print(f"  [{ACCENT}]{RAIL_TEE}{bottom_dashes}{RAIL_BOX_BR}[/{ACCENT}]")
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")


def rail_answer(value: str) -> None:
    """Render the selected answer below a step."""
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  {value}")


def rail_success(message: str) -> None:
    """Render a success message on the rail."""
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  [{SUCCESS}]✓ {message}[/{SUCCESS}]")


def rail_error(message: str) -> None:
    """Render an error message on the rail."""
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  [{ERROR}]✖ {message}[/{ERROR}]")


def rail_warning(message: str) -> None:
    """Render a warning message on the rail."""
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]  [{WARNING}]⚠ {message}[/{WARNING}]")


def rail_separator() -> None:
    """Render a blank rail line."""
    console.print(f"  [{ACCENT}]{RAIL_BAR}[/{ACCENT}]")


def rail_confirm(prompt_text: str, default: bool = True) -> bool:
    """Y/n prompt on the rail. Returns bool."""
    hint = "(Y/n)" if default else "(y/N)"
    console.print(f"\n  [{ACCENT}]{RAIL_STEP}[/{ACCENT}]  [bold]{prompt_text}[/bold] {hint}")
    try:
        answer = input(f"  {RAIL_BAR}  > ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        console.print()
        return default
    if not answer:
        return default
    return answer in ("y", "yes")


def rail_prompt(prompt_text: str, password: bool = False) -> str:
    """Text input on the rail."""
    console.print(f"\n  [{ACCENT}]{RAIL_STEP}[/{ACCENT}]  [bold]{prompt_text}[/bold]")
    prefix = f"  {RAIL_BAR}  > "
    try:
        if password:
            return getpass.getpass(prefix)
        return input(prefix).strip()
    except (EOFError, KeyboardInterrupt):
        console.print()
        return ""


def rail_summary_box(
    title: str, rows: list[tuple[str, str]], extra_lines: list[str] | None = None
) -> None:
    """Render a summary table in info-box style."""
    # Build content lines
    content_lines: list[str] = []
    for label, value in rows:
        content_lines.append(f"{label:<12}{value}")
    if extra_lines:
        content_lines.append("")
        content_lines.extend(extra_lines)

    rail_info_box(title, content_lines)
