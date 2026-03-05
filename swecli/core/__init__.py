"""Core functionality for OpenDev."""

import os
import warnings
from importlib import import_module
from typing import Dict, Tuple

# Suppress transformers warning about missing ML frameworks
# OpenDev uses LLM APIs directly and doesn't need local models
os.environ["TRANSFORMERS_VERBOSITY"] = "error"  # Only show errors, not warnings
warnings.filterwarnings("ignore", message=".*None of PyTorch, TensorFlow.*found.*")
warnings.filterwarnings("ignore", message=".*Models won't be available.*")

__all__ = [
    "ConfigManager",
    "SessionManager",
    "MainAgent",
    "ModeManager",
    "OperationMode",
    "ApprovalManager",
    "ApprovalChoice",
    "ApprovalResult",
    "ErrorHandler",
    "ErrorAction",
    "UndoManager",
    "ToolRegistry",
]

_EXPORTS: Dict[str, Tuple[str, str]] = {
    "MainAgent": ("swecli.core.agents", "MainAgent"),
    "ConfigManager": ("swecli.core.runtime", "ConfigManager"),
    "SessionManager": ("swecli.core.context_engineering.history", "SessionManager"),
    "ModeManager": ("swecli.core.runtime", "ModeManager"),
    "OperationMode": ("swecli.core.runtime", "OperationMode"),
    "UndoManager": ("swecli.core.context_engineering.history", "UndoManager"),
    "ApprovalManager": ("swecli.core.runtime.approval", "ApprovalManager"),
    "ApprovalChoice": ("swecli.core.runtime.approval", "ApprovalChoice"),
    "ApprovalResult": ("swecli.core.runtime.approval", "ApprovalResult"),
    "ErrorHandler": ("swecli.core.runtime.monitoring", "ErrorHandler"),
    "ErrorAction": ("swecli.core.runtime.monitoring", "ErrorAction"),
    "ToolRegistry": ("swecli.core.context_engineering.tools", "ToolRegistry"),
}


def __getattr__(name: str):
    if name not in _EXPORTS:
        raise AttributeError(f"module 'swecli.core' has no attribute '{name}'")
    module_path, attr_name = _EXPORTS[name]
    module = import_module(module_path)
    attr = getattr(module, attr_name)
    globals()[name] = attr
    return attr
