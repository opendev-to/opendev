#!/usr/bin/env python3
"""Test script to verify Textual runner works with tool calling."""

import os
from swecli.ui_textual.runner import launch_textual_cli

if __name__ == "__main__":
    print("=" * 80)
    print("OpenDev Textual UI Test")
    print("=" * 80)
    print()
    print("✅ Configuration Status:")

    # Check API key
    config_file = os.path.expanduser("~/.opendev/settings.json")
    if os.path.exists(config_file):
        print(f"   Config file: {config_file} (found)")
        import json
        with open(config_file) as f:
            config = json.load(f)
        model_provider = config.get("model_provider", "unknown")
        model = config.get("model", "unknown")
        print(f"   Model: {model_provider} / {model}")
    else:
        print(f"   Config file: {config_file} (NOT FOUND)")

    print()
    print("📝 Test Instructions:")
    print("   1. Type a simple message like 'hello' and press Enter")
    print("   2. The AI should respond (if API key is configured)")
    print("   3. Try 'create a test.txt file with hello world'")
    print("   4. An approval modal should appear for bash commands")
    print("   5. Press Ctrl+C to quit")
    print()
    print("🔍 Debug Mode: Enabled (will show message counts)")
    print("=" * 80)
    print()

    launch_textual_cli()
