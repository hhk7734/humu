from __future__ import annotations

from dataclasses import dataclass

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label


@dataclass
class RoomCreateResult:
    name: str


class CreateRoomScreen(ModalScreen[RoomCreateResult | None]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    DEFAULT_CSS = """
    CreateRoomScreen {
        align: center middle;
    }
    CreateRoomScreen > Vertical {
        width: 60;
        height: auto;
        border: thick $accent;
        padding: 1 2;
        background: $surface;
    }
    CreateRoomScreen Label {
        margin: 1 0 0 0;
    }
    CreateRoomScreen Input {
        margin: 0 0 1 0;
    }
    CreateRoomScreen Button {
        margin: 1 1 0 0;
    }
    """

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Room", id="title")
            yield Label("Room name:")
            yield Input(placeholder="design-review", id="room-name")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#room-name", Input).value.strip()
            if name:
                self.dismiss(RoomCreateResult(name=name))
            else:
                self.notify("Room name is required.", severity="error")
        else:
            self.dismiss(None)

    def action_cancel(self) -> None:
        self.dismiss(None)
