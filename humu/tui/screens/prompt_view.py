from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Static


class PromptViewScreen(ModalScreen[None]):
    BINDINGS = [("escape", "dismiss", "Close")]

    DEFAULT_CSS = """
    PromptViewScreen {
        align: center middle;
    }
    PromptViewScreen > VerticalScroll {
        width: 80%;
        height: 80%;
        border: thick $accent;
        padding: 1 2;
        background: $surface;
    }
    PromptViewScreen .title {
        text-style: bold;
        margin: 0 0 1 0;
    }
    PromptViewScreen .hint {
        text-style: italic;
        color: $text-muted;
        margin: 1 0 0 0;
    }
    """

    def __init__(self, agent_name: str, prompt: str) -> None:
        super().__init__()
        self._agent_name = agent_name
        self._prompt = prompt

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            yield Static(f"Prompt  [{self._agent_name}]", classes="title")
            yield Static(self._prompt)
            yield Static("Esc to close", classes="hint")
