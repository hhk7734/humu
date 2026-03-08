from __future__ import annotations

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Footer, Header, Label, ListView, Static, TextArea

from humu.client.connection import ServerConnection


class WorkspacePanel(Static):
    DEFAULT_CSS = "WorkspacePanel { width: 18; }"

    def compose(self) -> ComposeResult:
        yield Label("Workspaces", classes="panel-title")
        yield ListView(id="workspace-list")


class RoomPanel(Static):
    DEFAULT_CSS = "RoomPanel { width: 14; }"

    def compose(self) -> ComposeResult:
        yield Label("Rooms", classes="panel-title")
        yield ListView(id="room-list")


class ChatPanel(Static):
    DEFAULT_CSS = "ChatPanel { width: 1fr; }"

    def compose(self) -> ComposeResult:
        yield Label("Chat", classes="panel-title")
        yield Vertical(id="chat-messages")
        yield TextArea(id="chat-input")


class AgentPanel(Static):
    DEFAULT_CSS = "AgentPanel { width: 16; }"

    def compose(self) -> ComposeResult:
        yield Label("Agents", classes="panel-title")
        yield ListView(id="agent-list")


class HumuApp(App):
    CSS = """
    Screen {
        layout: horizontal;
    }
    .panel-title {
        text-style: bold;
        background: $accent;
        width: 100%;
        padding: 0 1;
    }
    #chat-messages {
        height: 1fr;
        overflow-y: auto;
    }
    #chat-input {
        height: 3;
        dock: bottom;
    }
    """

    TITLE = "Humu"

    def __init__(self) -> None:
        super().__init__()
        self._conn = ServerConnection(on_message=self._handle_server_event)

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal():
            yield WorkspacePanel()
            yield RoomPanel()
            yield ChatPanel()
            yield AgentPanel()
        yield Footer()

    def _handle_server_event(self, event: dict) -> None:
        # Will be expanded in subsequent tasks
        pass


def run_client() -> None:
    app = HumuApp()
    app.run()
