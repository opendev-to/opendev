#!/bin/bash
# Terminal information diagnostic

echo "=========================================="
echo "TERMINAL DIAGNOSTIC INFO"
echo "=========================================="
echo ""
echo "Terminal Emulator Information:"
echo "  TERM: $TERM"
echo "  TERM_PROGRAM: $TERM_PROGRAM"
echo "  TERM_PROGRAM_VERSION: $TERM_PROGRAM_VERSION"
echo "  COLORTERM: $COLORTERM"
echo "  Shell: $SHELL"
echo ""

# Test alternate screen support
echo "Testing alternate screen buffer support..."
echo ""
echo "Step 1: You should see this text"
echo "Step 2: Press Enter to switch to alternate screen"
read -p ""

# Enter alternate screen
tput smcup 2>/dev/null || printf '\033[?1049h'
clear

echo "=========================================="
echo "ALTERNATE SCREEN BUFFER"
echo "=========================================="
echo ""
echo "If this is working:"
echo "  - Previous text should be HIDDEN"
echo "  - Screen should be CLEAR"
echo ""
read -p "Press Enter to exit alternate screen..."

# Exit alternate screen
tput rmcup 2>/dev/null || printf '\033[?1049l'

echo ""
echo "Back to normal screen."
echo "Did the alternate screen work? (previous text was hidden)"
