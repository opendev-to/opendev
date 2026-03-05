"""Handler for ask user tool invocations."""

from __future__ import annotations

from typing import Any


class AskUserHandler:
    """Executes user question operations."""

    def __init__(self, ask_user_tool: Any = None):
        """Initialize the handler.

        Args:
            ask_user_tool: AskUserTool instance for asking questions
        """
        self._tool = ask_user_tool

    def ask_questions(self, args: dict[str, Any]) -> dict[str, Any]:
        """Handle user question requests.

        Args:
            args: Dictionary with:
                - questions: List of question dicts with:
                    - question: The question text
                    - header: Short label (max 12 chars)
                    - options: List of {label, description} dicts
                    - multiSelect: Whether to allow multiple selections

        Returns:
            Result dictionary with formatted output
        """
        if not self._tool:
            return {
                "success": False,
                "error": "AskUserTool not available",
                "output": None,
            }

        questions = args.get("questions", [])

        if not questions:
            return {
                "success": False,
                "error": "At least one question is required",
                "output": None,
            }

        # Perform the question asking
        result = self._tool.ask(questions)

        if not result.get("success"):
            return {
                "success": False,
                "error": result.get("error", "Unknown error"),
                "output": None,
            }

        if result.get("cancelled"):
            # Build questions context even for cancelled (for session persistence)
            questions_context = [
                {
                    "question": q.get("question"),
                    "header": q.get("header"),
                    "options": [
                        {"label": opt.get("label"), "description": opt.get("description", "")}
                        for opt in q.get("options", [])
                    ],
                    "multiSelect": q.get("multiSelect", False),
                }
                for q in questions
            ]
            return {
                "success": True,
                "output": "User cancelled the questions",
                "cancelled": True,
                "answers": {},
                "questions_context": questions_context,
            }

        # Format the answers for display
        answers = result.get("answers", {})
        total_questions = len(questions)
        answered_count = len(answers)

        # Build compact answer summary (single line for clean UI display)
        # Format: "Received 4/4 answers: [Header]=Answer, [Header2]=Answer2"
        answer_parts = []
        for idx, question in enumerate(questions):
            header = question.get("header") or f"Q{idx + 1}"  # Use Q# if header is empty/None
            answer = answers.get(str(idx), "(not answered)")
            answer_parts.append(f"[{header}]={answer}")

        output_text = f"Received {answered_count}/{total_questions} answers: " + ", ".join(
            answer_parts
        )

        # Build questions context for session persistence (enables display on resume)
        questions_context = [
            {
                "question": q.get("question"),
                "header": q.get("header"),
                "options": [
                    {"label": opt.get("label"), "description": opt.get("description", "")}
                    for opt in q.get("options", [])
                ],
                "multiSelect": q.get("multiSelect", False),
            }
            for q in questions
        ]

        return {
            "success": True,
            "output": output_text,
            "answers": answers,
            "cancelled": False,
            "questions_context": questions_context,
        }
