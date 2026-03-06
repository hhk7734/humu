from __future__ import annotations

import time

from textual.app import ComposeResult
from textual.message import Message
from textual.widgets import Label, ListItem, ListView, Static


class AgentEditRequested(Message):
    def __init__(self, name: str) -> None:
        super().__init__()
        self.name = name


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
    AgentPanel ListView > ListItem {
        height: 2;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__(**kwargs)
        self._last_selected_name: str | None = None
        self._last_selected_time: float = 0.0

    def compose(self) -> ComposeResult:
        yield Label("Agents", classes="panel-title")
        yield ListView(id="agent-list")

    def set_agents(
        self,
        leader: str | None,
        agents: list[str] | None = None,
        usage: dict[str, tuple[int, str]] | None = None,
    ) -> None:
        """Set agent list. *usage* maps agent name -> (total_tokens, model)."""
        lv = self.query_one("#agent-list", ListView)
        lv.clear()
        usage = usage or {}
        if leader:
            lv.append(ListItem(Label(self._format_label("*", leader, usage.get(leader))), name=leader))
        for name in agents or []:
            lv.append(ListItem(Label(self._format_label(" ", name, usage.get(name))), name=name))

    @staticmethod
    def _format_label(prefix: str, name: str, token_info: tuple[int, str] | None = None) -> str:
        if token_info:
            total_tokens, model = token_info
            if total_tokens > 0:
                from humu.config import MODEL_CONTEXT_WINDOWS, DEFAULT_CONTEXT_WINDOW
                ctx_size = MODEL_CONTEXT_WINDOWS.get(model, DEFAULT_CONTEXT_WINDOW)
                pct = min(total_tokens / ctx_size * 100, 100)
                return f"{prefix} {name} ({pct:.0f}%)"
        return f"{prefix} {name}"

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        name = event.item.name
        if not name:
            return
        now = time.monotonic()
        if name == self._last_selected_name and now - self._last_selected_time < 0.5:
            self.post_message(AgentEditRequested(name))
        self._last_selected_name = name
        self._last_selected_time = now
