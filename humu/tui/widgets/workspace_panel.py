from __future__ import annotations

from textual.app import ComposeResult
from textual.message import Message
from textual.widgets import Label, ListItem, ListView, Static


class WorkspaceSelected(Message):
    def __init__(self, name: str) -> None:
        super().__init__()
        self.name = name


class WorkspaceNewRequested(Message):
    pass


NEW_ITEM = "__new__"


class WorkspacePanel(Static):
    DEFAULT_CSS = """
    WorkspacePanel {
        width: 18;
        height: 100%;
        border: solid $accent;
    }
    WorkspacePanel ListView {
        height: 1fr;
    }
    WorkspacePanel .panel-title {
        text-style: bold;
        padding: 0 1;
        background: $accent;
        color: $text;
    }
    WorkspacePanel .new-item {
        color: $text-muted;
        text-style: italic;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self._workspaces: list[str] = []

    def compose(self) -> ComposeResult:
        yield Label("Workspace", classes="panel-title")
        yield ListView(id="workspace-list")

    def set_workspaces(self, names: list[str], selected: str | None = None) -> None:
        self._workspaces = names
        lv = self.query_one("#workspace-list", ListView)
        lv.clear()
        for name in names:
            prefix = "> " if name == selected else "  "
            lv.append(ListItem(Label(f"{prefix}{name}"), name=name))
        lv.append(ListItem(Label("+ new", classes="new-item"), name=NEW_ITEM))

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        if event.item.name == NEW_ITEM:
            self.post_message(WorkspaceNewRequested())
        elif event.item.name:
            self.post_message(WorkspaceSelected(event.item.name))
