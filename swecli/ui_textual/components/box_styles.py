"""Unified box and border styling for Textual components."""

from __future__ import annotations


class BoxStyles:
    """Centralized box drawing characters and color schemes."""

    BORDER_COLOR = "\033[38;5;240m"
    TITLE_COLOR = "\033[1;36m"
    ACCENT_COLOR = "\033[38;5;147m"
    SUCCESS_COLOR = "\033[1;32m"
    WARNING_COLOR = "\033[1;33m"
    ERROR_COLOR = "\033[1;31m"
    NORMAL_COLOR = "\033[38;5;250m"
    DIM_COLOR = "\033[38;5;240m"
    INFO_COLOR = "\033[38;5;117m"
    RESET = "\033[0m"

    TOP_LEFT = "╭"
    TOP_RIGHT = "╮"
    BOTTOM_LEFT = "╰"
    BOTTOM_RIGHT = "╯"
    HORIZONTAL = "─"
    VERTICAL = "│"
    LEFT_T = "├"
    RIGHT_T = "┤"
    TOP_T = "┬"
    BOTTOM_T = "┴"

    STANDARD_WIDTH = 80

    @classmethod
    def top_border(cls, width: int = STANDARD_WIDTH, title: str = "", colored: bool = True) -> str:
        border_color = cls.BORDER_COLOR if colored else ""
        reset = cls.RESET if colored else ""
        if title:
            title_section = f"─── {title} ─"
            remaining = width - len(title_section) - 2
            return f"{border_color}{cls.TOP_LEFT}{title_section}{cls.HORIZONTAL * max(0, remaining)}{cls.TOP_RIGHT}{reset}"
        return f"{border_color}{cls.TOP_LEFT}{cls.HORIZONTAL * (width - 2)}{cls.TOP_RIGHT}{reset}"

    @classmethod
    def bottom_border(cls, width: int = STANDARD_WIDTH, colored: bool = True) -> str:
        border_color = cls.BORDER_COLOR if colored else ""
        reset = cls.RESET if colored else ""
        return f"{border_color}{cls.BOTTOM_LEFT}{cls.HORIZONTAL * (width - 2)}{cls.BOTTOM_RIGHT}{reset}"

    @classmethod
    def separator(cls, width: int = STANDARD_WIDTH, colored: bool = True) -> str:
        border_color = cls.BORDER_COLOR if colored else ""
        reset = cls.RESET if colored else ""
        return f"{border_color}{cls.LEFT_T}{cls.HORIZONTAL * (width - 2)}{cls.RIGHT_T}{reset}"

    @classmethod
    def content_line(cls, content: str, width: int = STANDARD_WIDTH, padding: int = 1, colored: bool = True) -> str:
        import re

        border_color = cls.BORDER_COLOR if colored else ""
        reset = cls.RESET if colored else ""
        content_len = len(re.sub(r"\033\[[0-9;]+m", "", content))
        inner_width = width - 2 - padding * 2
        fill = max(0, inner_width - content_len)
        return (
            f"{border_color}{cls.VERTICAL}{reset}"
            f"{' ' * padding}{content}{' ' * fill}{' ' * padding}"
            f"{border_color}{cls.VERTICAL}{reset}"
        )

    @classmethod
    def empty_line(cls, width: int = STANDARD_WIDTH, colored: bool = True) -> str:
        return cls.content_line("", width=width, colored=colored)

    @classmethod
    def title_line(cls, title: str, width: int = STANDARD_WIDTH, centered: bool = True, colored: bool = True) -> str:
        title_color = cls.TITLE_COLOR if colored else ""
        reset = cls.RESET if colored else ""
        styled_title = f"{title_color}{title}{reset}"
        inner_width = width - 4
        title_len = len(title)
        if centered:
            left_pad = max(0, (inner_width - title_len) // 2)
            right_pad = max(0, inner_width - title_len - left_pad)
            content = f"{' ' * left_pad}{styled_title}{' ' * right_pad}"
        else:
            content = styled_title
        return cls.content_line(content, width=width, colored=colored)
