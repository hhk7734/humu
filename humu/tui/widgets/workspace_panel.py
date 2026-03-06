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
    WorkspacePanel ListView > ListItem {
        height: 2;
    }
    WorkspacePanel ListView > ListItem.--highlight {
        background: $accent 25%;
    }
    WorkspacePanel ListView > ListItem.selected {
        background: $accent 20%;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)
        self._workspaces: list[str] = []

    def compose(self) -> ComposeResult:
        yield Label("Workspaces", classes="panel-title")
        yield ListView(id="workspace-list")

    def set_workspaces(
        self,
        names: list[str],
        selected: str | None = None,
        processing: set[str] | None = None,
        spinner: str = "⠿",
    ) -> None:
        self._workspaces = names
        processing = processing or set()
        lv = self.query_one("#workspace-list", ListView)
        lv.clear()
        for name in names:
            prefix = "> " if name == selected else "  "
            badge = f" [yellow]{spinner}[/yellow]" if name in processing else ""
            item = ListItem(Label(f"{prefix}{name}{badge}"), name=name)
            if name == selected:
                item.add_class("selected")
            lv.append(item)

    def update_spinner(
        self,
        processing: set[str],
        spinner: str,
        selected: str | None = None,
    ) -> None:
        """Update only the spinner badge on existing items without rebuilding the list."""
        lv = self.query_one("#workspace-list", ListView)
        for item in lv.query(ListItem):
            if not item.name:
                continue
            labels = item.query(Label)
            if not labels:
                continue
            prefix = "> " if item.name == selected else "  "
            badge = f" [yellow]{spinner}[/yellow]" if item.name in processing else ""
            labels.first().update(f"{prefix}{item.name}{badge}")

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        if event.item.name:
            self.post_message(WorkspaceSelected(event.item.name))
