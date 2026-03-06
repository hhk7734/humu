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
    RoomPanel ListView > ListItem {
        height: 2;
    }
    RoomPanel ListView > ListItem.--highlight {
        background: $accent 25%;
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

    def set_rooms(
        self,
        names: list[str],
        selected: str | None = None,
        processing: set[str] | None = None,
        spinner: str = "⠿",
    ) -> None:
        self._rooms = names
        processing = processing or set()
        lv = self.query_one("#room-list", ListView)
        lv.clear()
        for name in names:
            prefix = "> " if name == selected else "  "
            badge = f" [yellow]{spinner}[/yellow]" if name in processing else ""
            lv.append(ListItem(Label(f"{prefix}{name}{badge}"), name=name))
        lv.append(ListItem(Label("+ new", classes="new-item"), name=NEW_ITEM))

    def update_spinner(
        self,
        processing: set[str],
        spinner: str,
        selected: str | None = None,
    ) -> None:
        """Update only the spinner badge on existing items without rebuilding the list."""
        lv = self.query_one("#room-list", ListView)
        for item in lv.query(ListItem):
            if not item.name or item.name == NEW_ITEM:
                continue
            labels = item.query(Label)
            if not labels:
                continue
            prefix = "> " if item.name == selected else "  "
            badge = f" [yellow]{spinner}[/yellow]" if item.name in processing else ""
            labels.first().update(f"{prefix}{item.name}{badge}")

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        if event.item.name == NEW_ITEM:
            self.post_message(RoomNewRequested())
        elif event.item.name:
            self.post_message(RoomSelected(event.item.name))
