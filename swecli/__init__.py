"""OpenDev - AI-powered command-line tool for accelerated development workflows."""

import warnings

# Suppress warnings from third-party dependencies
warnings.filterwarnings("ignore", message=".*None of PyTorch, TensorFlow.*found.*")  # transformers
warnings.filterwarnings("ignore", category=DeprecationWarning, module="crawl4ai")  # crawl4ai/pydantic

__version__ = "0.1.0"
