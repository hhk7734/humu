from __future__ import annotations

from textual.app import ComposeResult
from textual.message import Message
from textual.widgets import Label, ListItem, ListView, Static


class RoomSelected(Message):
    def __init__(self, name: str) -> None:
        super().__init__()
        self.name = name


class RoomNewRequested(Message):
    pass


NEW_ITEM = "__new__"


class RoomPanel(Static):
    DEFAULT_CSS = """
    RoomPanel {
        width: 14;
        height: 100%;
        border: solid $accent;
    }
    RoomPanel ListView {
        height: 1fr;
    }
    RoomPanel .panel-title {
        text-style: bold;
        padding: 0 1;
        background: $accent;
        color: $text;
    }
    RoomPanel .new-item {
        color: $text-muted;
        text-style: italic;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)
        self._rooms: list[str] = []

    def compose(self) -> ComposeResult:
        yield Label("Rooms", classes="panel-title")
        yield ListView(id="room-list")

    def set_rooms(self, names: list[str], selected: str | None = None) -> None:
        self._rooms = names
        lv = self.query_one("#room-list", ListView)
        lv.clear()
        for name in names:
            prefix = "> " if name == selected else "  "
            lv.append(ListItem(Label(f"{prefix}{name}"), name=name))
        lv.append(ListItem(Label("+ new", classes="new-item"), name=NEW_ITEM))

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        if event.item.name == NEW_ITEM:
            self.post_message(RoomNewRequested())
        elif event.item.name:
            self.post_message(RoomSelected(event.item.name))
