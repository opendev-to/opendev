"""Shared styling tokens for Textual UI surfaces."""

# =============================================================================
# Colors - All UI colors should be defined here for consistency
# =============================================================================

# Core semantic colors
PRIMARY = "#d0d4dc"
ACCENT = "#82a0ff"
SUBTLE = "#9aa0ac"
ERROR = "#ff5c57"
WARNING = "#ffb347"
SUCCESS = "#6ad18f"
GREY = "#7a7e86"
PANEL_BORDER = "#3a3f4b"

# Blues
BLUE_LIGHT = "#9ccffd"  # Headers, subtitles
BLUE_BRIGHT = "#4a9eff"  # Repo display, spinner text
BLUE_PATH = "#58a6ff"  # File paths in terminal
BLUE_BG_ACTIVE = "#1f2d3a"  # Active row background
BLUE_TASK = "#2596be"  # Background task indicator

# Greens
GREEN_BRIGHT = "#00ff00"  # Auto mode, success indicators
GREEN_LIGHT = "#89d185"  # Plan mode indicator
GREEN_PROMPT = "#7ee787"  # Terminal prompt ($)
TEXT_MUTED = "#5dbd61"  # Inline code (darker green)

# Oranges
ORANGE = "#ff8c00"  # Normal mode indicator
ORANGE_CAUTION = "#ffa500"  # Manual autonomy

# Cyans
CYAN = "#00bfff"  # Semi-auto mode
CYAN_VISION = "#00CED1"  # Vision model indicator

# Other
GOLD = "#FFD700"  # Thinking model indicator

# Thinking mode
THINKING = "#5a5e66"  # Darker gray for thinking content
THINKING_ICON = "⟡"  # Concave diamond - thinking prefix icon

# Prompt toolkit / toolbar colors
PT_ORANGE = "#ff9f43"  # Normal mode toolbar
PT_GREEN = "#2ecc71"  # Plan mode toolbar
PT_GREY = "#aaaaaa"  # Toolbar text
PT_PURPLE = "#6c5ce7"  # Model name
PT_BG_BLACK = "#000000"  # Menu background
PT_BG_SELECTED = "#2A2A2A"  # Selected completion
PT_META_GREY = "#808080"  # Meta text, keyboard shortcuts

# Menu/UI colors
MENU_HINT = "#7a8691"  # Menu hints/instructions

# Progress bar / animation
DIM_GREY = "#6b7280"  # Resting pearl color

# Animation gradient (green pulse for tool spinner)
GREEN_GRADIENT = [
    "#00ff00",
    "#00f500",
    "#00eb00",
    "#00e100",
    "#00d700",
    "#00cd00",
    "#00c300",
    "#00b900",
    "#00af00",
    "#00a500",
    "#009b00",
    "#009100",
    "#009b00",
    "#00a500",
    "#00af00",
    "#00b900",
    "#00c300",
    "#00cd00",
    "#00d700",
    "#00e100",
    "#00eb00",
    "#00f500",
    "#00ff00",
]

# Icons/prefixes
ERROR_ICON = "✖"
WARNING_ICON = "⚠"
SUCCESS_ICON = "✓"
HINT_ICON = "›"
INLINE_ARROW = "⎿"

# Railway wizard icons
RAIL_STEP = "◇"
RAIL_BAR = "│"
RAIL_START = "┌"
RAIL_END = "└"
RAIL_TEE = "├"
RAIL_DASH = "─"
RAIL_BOX_TR = "╮"
RAIL_BOX_BR = "╯"

# Tool icons
TOOL_ICONS = {
    "write_file": "+",
    "edit_file": "~",
    "read_file": ">",
    "list_directory": "/",
    "delete_file": "-",
    "run_command": "$",
}

# Status icons
STATUS_ICONS = {
    "success": "✓",
    "error": "✗",
    "warning": "⚠",
    "info": "ℹ",
}

# Common helper strings
UNKNOWN_COMMAND_HINT = "Type /help for available commands"
