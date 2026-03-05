"""Just-in-time context retrieval for proactive code loading."""

from __future__ import annotations

import re

from pathlib import Path
from typing import Any, Dict, List, Optional


class EntityExtractor:
    """Extract entities (files, functions, classes) from user input."""

    FILE_EXTENSIONS = {
        "py",
        "js",
        "ts",
        "jsx",
        "tsx",
        "java",
        "cpp",
        "c",
        "h",
        "hpp",
        "go",
        "rs",
        "rb",
        "php",
        "swift",
        "kt",
        "cs",
        "r",
        "m",
        "scala",
        "sh",
        "bash",
        "zsh",
        "yaml",
        "yml",
        "json",
        "toml",
        "xml",
        "html",
        "css",
        "scss",
        "sass",
        "md",
        "txt",
        "sql",
        "dockerfile",
        "makefile",
    }

    PATTERNS = {
        "file_path": r"\b[\w\-_./]+\.(?:" + "|".join(FILE_EXTENSIONS) + r")\b",
        "function": r"\b(?:[a-z_][a-z0-9_]*|[a-z][a-zA-Z0-9]*)\s*\(",
        "class": r"\b[A-Z][a-zA-Z0-9]*(?:Error|Exception|Manager|Service|Handler|Controller|Model|View|Component)?\b",
        "variable": r"\b(?:var|let|const|self|this)\s+([a-z_][a-z0-9_]*)\b",
        "action": r"\b(fix|debug|implement|create|add|remove|delete|update|modify|refactor|test|check|verify|optimize)\b",
    }

    def extract_entities(self, user_input: str) -> Dict[str, List[str]]:
        entities: Dict[str, List[str]] = {
            "files": [],
            "functions": [],
            "classes": [],
            "variables": [],
            "actions": [],
        }

        file_matches = re.finditer(self.PATTERNS["file_path"], user_input, re.IGNORECASE)
        entities["files"] = [m.group(0) for m in file_matches]

        func_matches = re.finditer(self.PATTERNS["function"], user_input)
        entities["functions"] = [m.group(0).rstrip("(").strip() for m in func_matches]

        class_matches = re.finditer(self.PATTERNS["class"], user_input)
        entities["classes"] = [m.group(0) for m in class_matches]

        var_matches = re.finditer(self.PATTERNS["variable"], user_input)
        entities["variables"] = [m.group(1) for m in var_matches]

        action_matches = re.finditer(self.PATTERNS["action"], user_input, re.IGNORECASE)
        entities["actions"] = [m.group(1).lower() for m in action_matches]

        for key in entities:
            entities[key] = list(set(entities[key]))

        return entities


class ContextRetriever:
    """Retrieve relevant context based on user intent."""

    def __init__(self, working_dir: Optional[Path] = None) -> None:
        self.working_dir = Path(working_dir or Path.cwd())
        self.extractor = EntityExtractor()

    def retrieve_context(self, user_input: str, max_files: int = 10) -> Dict[str, Any]:
        entities = self.extractor.extract_entities(user_input)
        context: Dict[str, Any] = {
            "entities": entities,
            "files_found": [],
            "patterns_found": [],
            "suggestions": [],
        }

        for file_path in entities["files"]:
            resolved = self._resolve_file_path(file_path)
            if resolved:
                context["files_found"].append(
                    {
                        "path": str(resolved),
                        "reason": "direct_mention",
                        "entity": file_path,
                    }
                )

        search_terms = entities["functions"] + entities["classes"]
        for term in search_terms:
            matches = self._grep_pattern(term, limit=5)
            for match in matches:
                if match not in [f["path"] for f in context["files_found"]]:
                    context["files_found"].append(
                        {
                            "path": match,
                            "reason": "contains_entity",
                            "entity": term,
                        }
                    )

        if "fix" in entities["actions"] or "debug" in entities["actions"]:
            context["suggestions"].append("Consider checking test files and error logs")

        if "implement" in entities["actions"] or "create" in entities["actions"]:
            context["suggestions"].append("Consider checking similar implementations")

        context["files_found"] = context["files_found"][:max_files]
        return context

    def _resolve_file_path(self, file_path: str) -> Optional[Path]:
        path = self.working_dir / file_path
        if path.exists():
            return path

        # Use native python traversal instead of subprocess find
        target_name = Path(file_path).name
        try:
            for item in self.working_dir.rglob(target_name):
                if item.is_file():
                    return item
        except Exception:
            pass

        return None

    def _grep_pattern(self, pattern: str, limit: int = 5) -> List[str]:
        try:
            result = subprocess.run(
                ["rg", "-l", pattern, str(self.working_dir)],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode != 0:
                result = subprocess.run(
                    ["grep", "-r", "-l", pattern, str(self.working_dir)],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )

            if result.returncode == 0:
                matches = [line.strip() for line in result.stdout.strip().split("\n") if line]
                return matches[:limit]
        except Exception:
            pass
        return []
