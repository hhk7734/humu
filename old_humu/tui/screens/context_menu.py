from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button


class ContextMenuScreen(ModalScreen[str | None]):
    BINDINGS = [("escape", "cancel", "Close")]

    DEFAULT_CSS = """
    ContextMenuScreen {
        align: center middle;
    }
    ContextMenuScreen > Vertical {
        width: auto;
        min-width: 24;
        height: auto;
        border: tall $accent;
        background: $surface;
        padding: 1 2;
    }
    ContextMenuScreen Button {
        width: 100%;
    }
    """

    def __init__(self, options: list[tuple[str, str]]) -> None:
        super().__init__()
        self._options = options

    def compose(self) -> ComposeResult:
        with Vertical():
            for key, label in self._options:
                yield Button(label, id=f"ctx-{key}")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        btn_id = event.button.id
        if btn_id and btn_id.startswith("ctx-"):
            self.dismiss(btn_id.removeprefix("ctx-"))

    def action_cancel(self) -> None:
        self.dismiss(None)
