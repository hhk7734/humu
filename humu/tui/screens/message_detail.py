from __future__ import annotations

import difflib
import json

from rich.json import JSON
from rich.syntax import Syntax

from textual.app import ComposeResult
from textual.containers import VerticalScroll
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
    MessageDetailScreen .hint {
        text-style: italic;
        color: $text-muted;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .no-log {
        color: $text-muted;
        text-style: italic;
        margin: 1 0;
    }
    MessageDetailScreen .step-badge {
        text-style: bold;
        margin: 1 0 0 0;
    }
    MessageDetailScreen .step-badge-thinking {
        color: $warning;
    }
    MessageDetailScreen .step-badge-tool {
        color: $accent;
    }
    MessageDetailScreen .step-badge-result {
        color: $success;
    }
    MessageDetailScreen .step-badge-error {
        color: $error;
    }
    MessageDetailScreen .step-badge-progress {
        color: $text-muted;
    }
    MessageDetailScreen .step-body {
        width: 1fr;
        height: auto;
        margin: 0 0 0 2;
        padding: 0 1;
        background: $background;
    }
    MessageDetailScreen .step-text {
        width: 1fr;
        height: auto;
        margin: 0 0 0 2;
        color: $text-muted;
    }
    MessageDetailScreen .divider {
        color: $text-muted;
        margin: 0 0 1 0;
    }
    MessageDetailScreen .step-kv {
        width: 1fr;
        height: auto;
        margin: 0 0 0 2;
        color: $text;
    }
    MessageDetailScreen .result-badge {
        text-style: bold;
        margin: 0 0 0 2;
    }
    MessageDetailScreen .result-badge-ok {
        color: $success;
    }
    MessageDetailScreen .result-badge-err {
        color: $error;
    }
    MessageDetailScreen .result-body {
        width: 1fr;
        height: auto;
        margin: 0 0 0 4;
        padding: 0 1;
        background: $background;
    }
    MessageDetailScreen .result-text {
        width: 1fr;
        height: auto;
        margin: 0 0 0 4;
        color: $text-muted;
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
        self._steps = steps or []

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            yield Static(f"Process Log  [{self._sender}]", classes="divider")

            if not self._steps:
                yield Static("No process log available.", classes="no-log")
            else:
                # Pre-build tool_use_id → result map for inline pairing
                result_map: dict[str, dict] = {
                    step["tool_use_id"]: step
                    for step in self._steps
                    if step.get("type") == "tool_result" and step.get("tool_use_id")
                }
                paired_ids: set[str] = set()

                step_num = 0
                for step in self._steps:
                    step_type = step.get("type", "unknown")

                    # tool_result will be rendered inline after its tool_use
                    if step_type == "tool_result":
                        if step.get("tool_use_id") in paired_ids:
                            continue
                        # Orphaned result — show standalone
                        step_num += 1
                        yield from self._render_result_badge(
                            step, step_num, indent=False
                        )
                        continue

                    step_num += 1

                    if step_type == "thinking":
                        yield Static(
                            f"[{step_num}] 💭 Thinking",
                            classes="step-badge step-badge-thinking",
                        )
                        yield Static(step.get("content", ""), classes="step-text")

                    elif step_type == "tool_use":
                        tool_name = step.get("name", "?")
                        yield Static(
                            f"[{step_num}] 🔧 Tool Call  {tool_name}",
                            classes="step-badge step-badge-tool",
                        )
                        tool_input = step.get("input", {})
                        if tool_name == "Edit":
                            yield from self._render_edit_input(tool_input)
                        elif tool_name == "Bash":
                            yield from self._render_bash_input(tool_input)
                        elif tool_name == "StructuredOutput":
                            yield from self._render_structured_output_input(tool_input)
                        else:
                            try:
                                yield Static(
                                    JSON(json.dumps(tool_input, ensure_ascii=False)),
                                    classes="step-body",
                                )
                            except Exception:
                                yield Static(str(tool_input), classes="step-text")

                        # Immediately show the paired result
                        tool_id = step.get("id", "")
                        if tool_id and tool_id in result_map:
                            paired_ids.add(tool_id)
                            yield from self._render_result_badge(
                                result_map[tool_id], indent=True
                            )

                    elif step_type == "task_progress":
                        tool = step.get("tool", "")
                        desc = step.get("description", "")
                        badge = f"[{step_num}] ⟳ Progress"
                        if tool:
                            badge += f"  {tool}"
                        yield Static(badge, classes="step-badge step-badge-progress")
                        if desc:
                            yield Static(desc, classes="step-text")

            yield Static("Esc to close", classes="hint")

    def _render_result_badge(self, step: dict, step_num: int = 0, indent: bool = True):
        """Render a tool_result entry, optionally indented (when paired inline)."""
        is_error = step.get("is_error", False)
        content = step.get("content", "")
        body_cls = "result-body" if indent else "step-body"
        text_cls = "result-text" if indent else "step-text"
        if is_error:
            badge_cls = (
                "result-badge result-badge-err"
                if indent
                else "step-badge step-badge-error"
            )
            label = "✗ Error" if indent else f"[{step_num}] ✗ Tool Error"
        else:
            badge_cls = (
                "result-badge result-badge-ok"
                if indent
                else "step-badge step-badge-result"
            )
            label = "✓ Result" if indent else f"[{step_num}] ✓ Tool Result"
        yield Static(label, classes=badge_cls)
        try:
            parsed = json.loads(content)
            yield Static(JSON(json.dumps(parsed, ensure_ascii=False)), classes=body_cls)
        except Exception:
            yield Static(content, classes=text_cls)

    def _render_structured_output_input(self, tool_input: dict):
        """Render StructuredOutput tool: show message as plain text, others as key:value."""
        MESSAGE_KEY = "message"
        for key, value in tool_input.items():
            if key == MESSAGE_KEY:
                continue
            yield Static(f"{key}: {value}", classes="step-kv")

        message = tool_input.get(MESSAGE_KEY, "")
        if message:
            yield Static(message, classes="step-text")

    def _render_bash_input(self, tool_input: dict):
        """Render Bash tool input: syntax-highlighted command + key:value for other fields."""
        COMMAND_KEY = "command"
        for key, value in tool_input.items():
            if key == COMMAND_KEY:
                continue
            yield Static(f"{key}: {value}", classes="step-kv")

        command = tool_input.get(COMMAND_KEY, "")
        if command:
            yield Static(
                Syntax(command, "bash", theme="ansi_dark"),
                classes="step-body",
            )

    def _render_edit_input(self, tool_input: dict):
        """Render Edit tool input: key:value for scalar fields, unified diff for strings."""
        DIFF_KEYS = {"old_string", "new_string"}
        for key, value in tool_input.items():
            if key in DIFF_KEYS:
                continue
            yield Static(f"{key}: {value}", classes="step-kv")

        old_str = tool_input.get("old_string", "")
        new_str = tool_input.get("new_string", "")
        diff_lines = list(
            difflib.unified_diff(
                old_str.splitlines(keepends=True),
                new_str.splitlines(keepends=True),
                fromfile="old",
                tofile="new",
            )
        )
        if diff_lines:
            diff_text = "".join(diff_lines)
            yield Static(
                Syntax(diff_text, "diff", theme="ansi_dark"),
                classes="step-body",
            )
        elif old_str or new_str:
            # No diff (identical strings) — still show new_string
            yield Static(new_str, classes="step-text")
