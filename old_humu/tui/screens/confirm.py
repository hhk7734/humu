from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Label


class ConfirmScreen(ModalScreen[bool]):
    """Generic yes/no confirmation dialog."""

    BINDINGS = [
        Binding("escape", "dismiss_no", "Cancel"),
        Binding("enter", "dismiss_yes", "Confirm"),
    ]

    DEFAULT_CSS = """
    ConfirmScreen {
        align: center middle;
        background: $background 60%;
    }
    ConfirmScreen #dialog {
        width: 50;
        height: auto;
        border: solid $warning;
        background: $surface;
        padding: 1 2;
    }
    ConfirmScreen #message {
        margin-bottom: 1;
        text-align: center;
    }
    ConfirmScreen #actions {
        layout: horizontal;
        height: auto;
        align: center middle;
    }
    ConfirmScreen Button {
        margin: 0 1;
        width: 10;
    }
    """

    def __init__(self, message: str) -> None:
        super().__init__()
        self._message = message

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog"):
            yield Label(self._message, id="message")
            with Horizontal(id="actions"):
                yield Button("Delete", variant="error", id="btn-yes")
                yield Button("Cancel", variant="default", id="btn-no")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        self.dismiss(event.button.id == "btn-yes")

    def action_dismiss_yes(self) -> None:
        self.dismiss(True)

    def action_dismiss_no(self) -> None:
        self.dismiss(False)
