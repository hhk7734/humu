from __future__ import annotations

import logging

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Footer, Header, Label, ListItem, ListView, Static, TextArea

from humu.client.connection import ServerConnection
from humu.client.http import HttpClient

logger = logging.getLogger(__name__)


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
        self._http = HttpClient()
        self._current_workspace: str | None = None
        self._current_room: str | None = None

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal():
            yield WorkspacePanel()
            yield RoomPanel()
            yield ChatPanel()
            yield AgentPanel()
        yield Footer()

    async def on_mount(self) -> None:
        try:
            await self._http.start()
        except Exception:
            logger.exception("Failed to start HTTP client")
            return
        self.run_worker(self._ws_loop(), exclusive=True, group="ws")
        await self._load_workspaces()

    async def _ws_loop(self) -> None:
        try:
            await self._conn.connect()
            await self._conn.receive_loop()
        except Exception:
            logger.exception("WebSocket connection failed")

    async def on_unmount(self) -> None:
        await self._conn.disconnect()
        await self._http.stop()

    # --- Data Loading ---

    async def _load_workspaces(self) -> None:
        try:
            workspaces = await self._http.list_workspaces()
        except Exception:
            logger.exception("Failed to load workspaces")
            return
        lv = self.query_one("#workspace-list", ListView)
        await lv.clear()
        for ws in workspaces:
            item = ListItem(Label(ws["name"]))
            item.data = ws["name"]
            await lv.append(item)

    async def _load_rooms(self, workspace: str) -> None:
        try:
            rooms = await self._http.list_rooms(workspace)
        except Exception:
            logger.exception("Failed to load rooms")
            return
        lv = self.query_one("#room-list", ListView)
        await lv.clear()
        for room in rooms:
            item = ListItem(Label(room["name"]))
            item.data = room["name"]
            await lv.append(item)

    async def _load_agents(self, workspace: str, room: str) -> None:
        try:
            agents = await self._http.list_agents(workspace, room)
        except Exception:
            logger.exception("Failed to load agents")
            return
        # Find room leader
        try:
            rooms = await self._http.list_rooms(workspace)
            leader_name = None
            for r in rooms:
                if r["name"] == room:
                    leader_name = r.get("leader")
                    break
        except Exception:
            leader_name = None

        lv = self.query_one("#agent-list", ListView)
        await lv.clear()
        for agent in agents:
            name = agent["name"]
            prefix = "* " if name == leader_name else ""
            model = agent.get("model", "")
            label = f"{prefix}{name}\n[dim]{model}[/dim]"
            item = ListItem(Label(label, markup=True))
            item.data = agent["name"]
            await lv.append(item)

    async def _clear_rooms(self) -> None:
        lv = self.query_one("#room-list", ListView)
        await lv.clear()

    async def _clear_agents(self) -> None:
        lv = self.query_one("#agent-list", ListView)
        await lv.clear()

    async def _clear_chat(self) -> None:
        container = self.query_one("#chat-messages", Vertical)
        await container.remove_children()

    # --- Panel Selection ---

    async def on_list_view_selected(self, event: ListView.Selected) -> None:
        item = event.item
        name = getattr(item, "data", None)
        if name is None:
            return

        list_view = event.list_view
        list_id = list_view.id

        if list_id == "workspace-list":
            self._current_workspace = name
            self._current_room = None
            await self._clear_agents()
            await self._clear_chat()
            await self._load_rooms(name)

        elif list_id == "room-list" and self._current_workspace:
            self._current_room = name
            await self._load_agents(self._current_workspace, name)
            await self._conn.subscribe_room(self._current_workspace, name)

    # --- Server Events ---

    def _handle_server_event(self, event: dict) -> None:
        event_type = event.get("type", "")
        if event_type == "room_state_sync":
            self.call_from_thread(self._render_history, event.get("messages", []))
        elif event_type == "message_added":
            self.call_from_thread(
                self._append_message, event.get("sender", ""), event.get("text", "")
            )

    async def _render_history(self, messages: list[dict]) -> None:
        container = self.query_one("#chat-messages", Vertical)
        await container.remove_children()
        for msg in messages:
            sender = msg.get("sender", "unknown")
            text = msg.get("text", "")
            await container.mount(Static(f"[bold]{sender}[/bold]: {text}", markup=True))
        container.scroll_end(animate=False)

    async def _append_message(self, sender: str, text: str) -> None:
        container = self.query_one("#chat-messages", Vertical)
        await container.mount(Static(f"[bold]{sender}[/bold]: {text}", markup=True))
        container.scroll_end(animate=False)


def run_client() -> None:
    app = HumuApp()
    app.run()
