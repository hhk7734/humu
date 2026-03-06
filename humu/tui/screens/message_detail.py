from __future__ import annotations

import json

from rich.text import Text

from textual.app import ComposeResult
from textual.containers import Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Label, Static


class MessageDetailScreen(ModalScreen[None]):
    BINDINGS = [("escape", "dismiss", "Close")]

    DEFAULT_CSS = """
    MessageDetailScreen {
        align: center middle;
    }
    MessageDetailScreen > VerticalScroll {
        width: 80%;
        height: 80%;
        border: thick $accent;
        padding: 1 2;
        background: $surface;
    }
    MessageDetailScreen .detail-sender {
        text-style: bold;
        color: $accent;
        margin: 0 0 1 0;
    }
    MessageDetailScreen .detail-sender-system {
        text-style: italic;
        color: $text-muted;
        margin: 0 0 1 0;
    }
    MessageDetailScreen .detail-sender-error {
        text-style: bold;
        color: $error;
        margin: 0 0 1 0;
    }
    MessageDetailScreen .detail-body {
        width: 1fr;
        height: auto;
    }
    MessageDetailScreen .detail-raw-header {
        text-style: bold;
        color: $warning;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .detail-raw {
        width: 1fr;
        height: auto;
        color: $text-muted;
        margin: 0 0 0 2;
    }
    MessageDetailScreen .detail-hint {
        text-style: italic;
        color: $text-muted;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .step-header {
        text-style: bold;
        color: $warning;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .step-label {
        text-style: bold;
        color: $accent;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .step-body {
        width: 1fr;
        height: auto;
        color: $text-muted;
        margin: 0 0 0 2;
    }
    MessageDetailScreen .step-error {
        color: $error;
        margin: 0 0 0 2;
    }
    """

    def __init__(
        self,
        sender: str,
        text: str,
        is_system: bool = False,
        raw: str | None = None,
        steps: list[dict] | None = None,
    ) -> None:
        super().__init__()
        self._sender = sender
        self._text = text
        self._is_system = is_system
        self._raw = raw
        self._steps = steps or []

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            if self._sender == "error":
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender-error")
            elif self._is_system:
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender-system")
            else:
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender")

            if self._text:
                yield Static(self._text, classes="detail-body")

            if self._raw and self._raw != self._text:
                yield Label("Raw response:", classes="detail-raw-header")
                yield Static(self._raw, classes="detail-raw")

            if self._steps:
                yield Label("─── Process Log ───", classes="step-header")
                for i, step in enumerate(self._steps, 1):
                    step_type = step.get("type", "unknown")
                    if step_type == "thinking":
                        yield Label(f"[{i}] Thinking", classes="step-label")
                        yield Static(step.get("content", ""), classes="step-body")
                    elif step_type == "tool_use":
                        tool_name = step.get("name", "?")
                        tool_input = step.get("input", {})
                        yield Label(f"[{i}] Tool Call: {tool_name}", classes="step-label")
                        try:
                            input_str = json.dumps(tool_input, indent=2, ensure_ascii=False)
                        except Exception:
                            input_str = str(tool_input)
                        yield Static(input_str, classes="step-body")
                    elif step_type == "tool_result":
                        is_error = step.get("is_error", False)
                        content = step.get("content", "")
                        label_cls = "step-error" if is_error else "step-label"
                        status = "Error" if is_error else "Result"
                        yield Label(f"[{i}] Tool {status}", classes=label_cls)
                        yield Static(content, classes="step-body")
                    elif step_type == "task_progress":
                        desc = step.get("description", "")
                        tool = step.get("tool", "")
                        suffix = f" ({tool})" if tool else ""
                        yield Label(f"[{i}] Progress{suffix}", classes="step-label")
                        yield Static(desc, classes="step-body")

            yield Label("Press Esc to close", classes="detail-hint")
