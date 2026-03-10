.PHONY: help install install-ui format lint typecheck test test-file test-cov check build-ui

PYTHON_DIRS = opendev/ tests/
LINE_LENGTH = 100

help:
	@echo "Available commands:"
	@echo "  make install      Install with dev dependencies"
	@echo "  make format       Format code with Black"
	@echo "  make lint         Lint with Ruff (auto-fix)"
	@echo "  make typecheck    Type-check with mypy"
	@echo "  make check        Run format + lint + typecheck"
	@echo "  make test         Run all tests"
	@echo "  make test-cov     Run tests with coverage"
	@echo "  make install-ui   Install web UI npm dependencies"
	@echo "  make build-ui     Build web UI frontend"

install:
	uv venv && uv pip install -e ".[dev]"

format:
	black $(PYTHON_DIRS) --line-length $(LINE_LENGTH)

lint:
	ruff check $(PYTHON_DIRS) --fix

typecheck:
	mypy opendev/

check: format lint typecheck

test:
	uv run pytest

test-cov:
	uv run pytest --cov=opendev

# Usage: make test-file FILE=tests/test_session_manager.py
test-file:
	uv run pytest $(FILE)

install-ui:
	cd web-ui && npm ci

build-ui: install-ui
	cd web-ui && npm run build
