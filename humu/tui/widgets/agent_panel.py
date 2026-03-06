from __future__ import annotations

from textual.app import ComposeResult
from textual.widgets import Label, ListItem, ListView, Static


class AgentPanel(Static):
    DEFAULT_CSS = """
    AgentPanel {
        width: 16;
        height: 100%;
        border: solid $accent;
    }
    AgentPanel ListView {
        height: 1fr;
    }
    AgentPanel .panel-title {
        text-style: bold;
        padding: 0 1;
        background: $accent;
        color: $text;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)

    def compose(self) -> ComposeResult:
        yield Label("Agents", classes="panel-title")
        yield ListView(id="agent-list")

    def set_agents(self, leader: str | None, agents: list[str] | None = None) -> None:
        lv = self.query_one("#agent-list", ListView)
        lv.clear()
        if leader:
            lv.append(ListItem(Label(f"* {leader}"), name=leader))
        for name in agents or []:
            lv.append(ListItem(Label(f"  {name}"), name=name))
