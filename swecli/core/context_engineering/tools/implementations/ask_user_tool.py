"""Tool for asking structured questions to the user."""

from __future__ import annotations

import logging
from dataclasses import dataclass
from typing import Any, Callable, Sequence

logger = logging.getLogger(__name__)


@dataclass
class QuestionOption:
    """An option for a question."""

    label: str
    description: str = ""


@dataclass
class Question:
    """A structured question for the user."""

    question: str
    header: str  # max 12 chars, displayed as chip/tag
    options: list[QuestionOption]
    multi_select: bool = False


class AskUserTool:
    """Tool for structured user questioning with multiple-choice options.

    This tool allows the agent to ask the user structured questions with
    predefined options. Users can select from the options or provide
    custom "Other" input.
    """

    def __init__(self, ui_prompt_callback: Callable | None = None):
        """Initialize ask user tool.

        Args:
            ui_prompt_callback: Callback function to display questions and get responses.
                                If None, falls back to console input.
        """
        self._prompt_callback = ui_prompt_callback

    def set_prompt_callback(self, callback: Callable) -> None:
        """Set the UI prompt callback after initialization.

        Args:
            callback: Callback function to display questions and get responses.
        """
        self._prompt_callback = callback

    def ask(
        self,
        questions: Sequence[dict[str, Any]],
    ) -> dict[str, Any]:
        """Ask user structured questions and return responses.

        Args:
            questions: List of question dictionaries, each with:
                - question: The question text
                - header: Short label (max 12 chars)
                - options: List of {label, description} dicts (2-4 options)
                - multiSelect: Whether multiple options can be selected

        Returns:
            Dictionary with:
            - success: bool
            - answers: dict mapping question index to selected answer(s)
            - cancelled: bool (True if user cancelled)
            - error: str | None
        """
        # Validate questions
        if not questions:
            return {
                "success": False,
                "error": "At least one question is required",
                "answers": {},
                "cancelled": False,
            }

        if len(questions) > 4:
            return {
                "success": False,
                "error": "Maximum 4 questions allowed",
                "answers": {},
                "cancelled": False,
            }

        # Validate each question
        parsed_questions = []
        for i, q in enumerate(questions):
            if not isinstance(q, dict):
                return {
                    "success": False,
                    "error": f"Question {i} must be a dictionary",
                    "answers": {},
                    "cancelled": False,
                }

            question_text = q.get("question", "")
            if not question_text:
                return {
                    "success": False,
                    "error": f"Question {i} is missing 'question' text",
                    "answers": {},
                    "cancelled": False,
                }

            header = q.get("header", "")[:12]  # Max 12 chars
            options = q.get("options", [])

            if len(options) < 2 or len(options) > 4:
                return {
                    "success": False,
                    "error": f"Question {i} must have 2-4 options, got {len(options)}",
                    "answers": {},
                    "cancelled": False,
                }

            parsed_options = []
            for opt in options:
                if isinstance(opt, dict):
                    parsed_options.append(QuestionOption(
                        label=opt.get("label", ""),
                        description=opt.get("description", ""),
                    ))
                else:
                    parsed_options.append(QuestionOption(label=str(opt)))

            parsed_questions.append(Question(
                question=question_text,
                header=header,
                options=parsed_options,
                multi_select=q.get("multiSelect", False),
            ))

        # If we have a UI callback, use it
        if self._prompt_callback:
            try:
                result = self._prompt_callback(parsed_questions)
                if result is None:
                    return {
                        "success": True,
                        "answers": {},
                        "cancelled": True,
                        "error": None,
                    }
                return {
                    "success": True,
                    "answers": result,
                    "cancelled": False,
                    "error": None,
                }
            except Exception as e:
                logger.exception("Error in UI prompt callback")
                return {
                    "success": False,
                    "error": f"UI error: {str(e)}",
                    "answers": {},
                    "cancelled": False,
                }

        # Fallback to console input if no UI callback
        return self._ask_console(parsed_questions)

    def _ask_console(self, questions: list[Question]) -> dict[str, Any]:
        """Fallback console-based question asking.

        Args:
            questions: List of parsed Question objects

        Returns:
            Result dictionary with answers
        """
        answers = {}

        for i, q in enumerate(questions):
            print(f"\n{q.header}: {q.question}")
            for j, opt in enumerate(q.options, 1):
                desc = f" - {opt.description}" if opt.description else ""
                print(f"  {j}. {opt.label}{desc}")
            print(f"  {len(q.options) + 1}. Other (custom input)")

            while True:
                try:
                    if q.multi_select:
                        response = input("Enter numbers (comma-separated) or 'c' to cancel: ").strip()
                    else:
                        response = input("Enter number or 'c' to cancel: ").strip()

                    if response.lower() == 'c':
                        return {
                            "success": True,
                            "answers": {},
                            "cancelled": True,
                            "error": None,
                        }

                    if q.multi_select:
                        selections = [int(x.strip()) for x in response.split(',')]
                        selected_answers = []
                        for sel in selections:
                            if sel == len(q.options) + 1:
                                custom = input("Enter custom answer: ").strip()
                                selected_answers.append(custom)
                            elif 1 <= sel <= len(q.options):
                                selected_answers.append(q.options[sel - 1].label)
                            else:
                                raise ValueError("Invalid selection")
                        answers[str(i)] = selected_answers
                    else:
                        selection = int(response)
                        if selection == len(q.options) + 1:
                            custom = input("Enter custom answer: ").strip()
                            answers[str(i)] = custom
                        elif 1 <= selection <= len(q.options):
                            answers[str(i)] = q.options[selection - 1].label
                        else:
                            print("Invalid selection, try again.")
                            continue
                    break

                except (ValueError, KeyError):
                    print("Invalid input, try again.")

        return {
            "success": True,
            "answers": answers,
            "cancelled": False,
            "error": None,
        }
