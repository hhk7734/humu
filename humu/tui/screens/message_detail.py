from __future__ import annotations

from rich.text import Text

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
    """

    def __init__(
        self,
        sender: str,
        text: str,
        is_system: bool = False,
        raw: str | None = None,
    ) -> None:
        super().__init__()
        self._sender = sender
        self._text = text
        self._is_system = is_system
        self._raw = raw

    def compose(self) -> ComposeResult:
        with VerticalScroll():
            if self._sender == "error":
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender-error")
            elif self._is_system:
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender-system")
            else:
                yield Label(Text(f"[{self._sender}]"), classes="detail-sender")
            yield Static(self._text, classes="detail-body")
            if self._raw and self._raw != self._text:
                yield Label("Raw response:", classes="detail-raw-header")
                yield Static(self._raw, classes="detail-raw")
            yield Label("Press Esc to close", classes="detail-hint")
