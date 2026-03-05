"""Modal screen for structured user questions."""

from __future__ import annotations

from typing import Any

from textual.app import ComposeResult
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Static, RadioButton, RadioSet, Checkbox


class QuestionScreen(ModalScreen[dict[str, Any] | None]):
    """Display structured questions with options.

    Shows multiple-choice questions with optional multi-select.
    Users can select from predefined options or provide custom "Other" input.
    """

    DEFAULT_CSS = """
    QuestionScreen > Container {
        width: 80;
        max-height: 90%;
        border: round $accent;
        background: $surface;
        padding: 1 2;
    }

    QuestionScreen .title {
        content-align: center middle;
        height: 1;
        color: $accent;
        text-style: bold;
        margin-bottom: 1;
    }

    QuestionScreen .question-container {
        margin-bottom: 1;
        padding: 1;
        border: round $primary;
        background: $boost;
    }

    QuestionScreen .question-header {
        color: $primary;
        text-style: bold;
        margin-bottom: 0;
    }

    QuestionScreen .question-text {
        color: $text;
        margin-bottom: 1;
    }

    QuestionScreen .option-description {
        color: $text-muted;
        padding-left: 3;
        margin-bottom: 0;
    }

    QuestionScreen .other-input {
        margin-top: 1;
        display: none;
    }

    QuestionScreen .other-input.visible {
        display: block;
    }

    QuestionScreen Horizontal {
        margin-top: 1;
        height: 3;
        content-align: center middle;
    }

    QuestionScreen Button {
        min-width: 12;
        margin: 0 1;
    }

    QuestionScreen RadioSet {
        border: none;
        padding: 0;
        margin: 0;
    }
    """

    def __init__(self, questions: list[Any]) -> None:
        """Initialize question screen.

        Args:
            questions: List of Question dataclass instances with:
                - question: Question text
                - header: Short label (max 12 chars)
                - options: List of QuestionOption(label, description)
                - multi_select: Whether to allow multiple selections
        """
        super().__init__()
        self.questions = questions
        self.answers: dict[str, Any] = {}
        self._radio_sets: list[RadioSet] = []
        self._checkboxes: dict[int, list[Checkbox]] = {}
        self._other_inputs: dict[int, Input] = {}
        self._other_checked: dict[int, bool] = {}

    def compose(self) -> ComposeResult:
        with Container():
            yield Static("Questions", classes="title")
            with VerticalScroll():
                for i, q in enumerate(self.questions):
                    with Container(classes="question-container", id=f"q-{i}"):
                        yield Static(f"[{q.header}]", classes="question-header")
                        yield Static(q.question, classes="question-text")

                        if q.multi_select:
                            # Multi-select: use checkboxes
                            self._checkboxes[i] = []
                            with Vertical():
                                for j, opt in enumerate(q.options):
                                    cb = Checkbox(opt.label, id=f"cb-{i}-{j}")
                                    self._checkboxes[i].append(cb)
                                    yield cb
                                    if opt.description:
                                        yield Static(
                                            opt.description,
                                            classes="option-description",
                                        )
                                # Add "Other" checkbox
                                other_cb = Checkbox("Other", id=f"cb-{i}-other")
                                self._checkboxes[i].append(other_cb)
                                yield other_cb
                        else:
                            # Single-select: use radio buttons
                            radio_set = RadioSet(id=f"radio-{i}")
                            self._radio_sets.append(radio_set)
                            with radio_set:
                                for j, opt in enumerate(q.options):
                                    yield RadioButton(opt.label, id=f"rb-{i}-{j}")
                                # Add "Other" option
                                yield RadioButton("Other", id=f"rb-{i}-other")

                        # "Other" text input (hidden by default)
                        other_input = Input(
                            placeholder="Enter custom answer...",
                            id=f"other-input-{i}",
                            classes="other-input",
                        )
                        self._other_inputs[i] = other_input
                        yield other_input

            with Horizontal():
                yield Button("Submit", id="submit", variant="primary")
                yield Button("Cancel", id="cancel", variant="error")

    def on_radio_set_changed(self, event: RadioSet.Changed) -> None:
        """Handle radio button selection changes."""
        radio_id = event.radio_set.id or ""
        if radio_id.startswith("radio-"):
            q_index = int(radio_id.replace("radio-", ""))
            # Check if "Other" is selected
            selected = event.pressed
            is_other = selected and selected.id and selected.id.endswith("-other")
            self._show_other_input(q_index, is_other)

    def on_checkbox_changed(self, event: Checkbox.Changed) -> None:
        """Handle checkbox selection changes."""
        cb_id = event.checkbox.id or ""
        if cb_id.startswith("cb-") and cb_id.endswith("-other"):
            # Extract question index
            parts = cb_id.split("-")
            if len(parts) >= 2:
                q_index = int(parts[1])
                self._show_other_input(q_index, event.value)

    def _show_other_input(self, q_index: int, show: bool) -> None:
        """Show or hide the 'Other' input field."""
        if q_index in self._other_inputs:
            input_widget = self._other_inputs[q_index]
            if show:
                input_widget.add_class("visible")
                input_widget.focus()
            else:
                input_widget.remove_class("visible")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "submit":
            self._collect_answers()
            self.dismiss(self.answers)
        elif event.button.id == "cancel":
            self.dismiss(None)

    def _collect_answers(self) -> None:
        """Collect all answers from the form."""
        for i, q in enumerate(self.questions):
            if q.multi_select:
                # Collect from checkboxes
                selected = []
                for j, cb in enumerate(self._checkboxes.get(i, [])):
                    if cb.value:
                        if cb.id and cb.id.endswith("-other"):
                            # Get custom value
                            other_input = self._other_inputs.get(i)
                            if other_input and other_input.value.strip():
                                selected.append(other_input.value.strip())
                        else:
                            selected.append(q.options[j].label)
                self.answers[str(i)] = selected
            else:
                # Collect from radio set
                for radio_set in self._radio_sets:
                    if radio_set.id == f"radio-{i}":
                        selected = radio_set.pressed_button
                        if selected:
                            if selected.id and selected.id.endswith("-other"):
                                # Get custom value
                                other_input = self._other_inputs.get(i)
                                if other_input and other_input.value.strip():
                                    self.answers[str(i)] = other_input.value.strip()
                                else:
                                    self.answers[str(i)] = ""
                            else:
                                # Extract option index from button id
                                parts = (selected.id or "").split("-")
                                if len(parts) >= 3:
                                    opt_index = int(parts[2])
                                    self.answers[str(i)] = q.options[opt_index].label
                        break

    def on_mount(self) -> None:
        """Focus the first input when mounted."""
        pass
