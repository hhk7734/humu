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
    PromptViewScreen .section {
        text-style: bold;
        color: $accent;
        margin: 1 0 0 0;
    }
    PromptViewScreen .hint {
        text-style: italic;
        color: $text-muted;
        margin: 1 0 0 0;
    }
    """

    def __init__(
        self,
        agent_name: str,
        system_prompt: str = "",
        user_message: str = "",
    ) -> None:
        super().__init__()
        self._agent_name = agent_name
        self._system_prompt = system_prompt
        self._user_message = user_message

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            yield Static(f"Prompt  [{self._agent_name}]", classes="title")
            if self._system_prompt:
                yield Static("System Prompt", classes="section")
                yield Static(self._system_prompt)
            if self._user_message:
                yield Static("User Message", classes="section")
                yield Static(self._user_message)
            if not self._system_prompt and not self._user_message:
                yield Static("No prompt data available.", classes="hint")
            yield Static("Esc to close", classes="hint")
