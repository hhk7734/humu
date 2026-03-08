from __future__ import annotations

import json
import logging

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.events import MouseDown, MouseMove, MouseUp
from textual.message import Message
from textual.widgets import Footer, Header, Label, ListItem, ListView, Static, TextArea

from humu.client.completers import ChatCompleter
from humu.client.connection import ServerConnection
from humu.client.http import HttpClient
from humu.config import CLIENT_STATE
from humu.client.screens import (
    ConfirmDeleteScreen,
    CreateAgentScreen,
    CreateRoomScreen,
    CreateWorkspaceScreen,
)

logger = logging.getLogger(__name__)


class WorkspacePanel(Static):
    DEFAULT_CSS = "WorkspacePanel { width: 18; }"

    def __init__(self, **kwargs) -> None:
        super().__init__(id="panel-workspace", **kwargs)

    def compose(self) -> ComposeResult:
        yield Label("Workspaces", classes="panel-title")
        yield ListView(id="workspace-list")


class RoomPanel(Static):
    DEFAULT_CSS = "RoomPanel { width: 14; }"

    def __init__(self, **kwargs) -> None:
        super().__init__(id="panel-room", **kwargs)

    def compose(self) -> ComposeResult:
        yield Label("Rooms", classes="panel-title")
        yield ListView(id="room-list")


class ChatInput(TextArea):
    """TextArea that emits Submitted on Enter instead of inserting newline."""

    class Submitted(Message):
        def __init__(self, text: str) -> None:
            super().__init__()
            self.text = text

    def _on_key(self, event) -> None:
        try:
            completer = self.screen.query_one("#chat-completer", ChatCompleter)
        except Exception:
            completer = None

        if completer and completer.is_active:
            if event.key == "down":
                completer.move_up()  # reversed for drop-up
                event.prevent_default()
                event.stop()
                return
            if event.key == "up":
                completer.move_down()  # reversed for drop-up
                event.prevent_default()
                event.stop()
                return
            if event.key == "tab":
                self._accept_completion(completer, add_space=False)
                event.prevent_default()
                event.stop()
                return
            if event.key == "enter":
                self._accept_completion(completer, add_space=True)
                completer.hide()
                event.prevent_default()
                event.stop()
                return
            if event.key == "escape":
                completer.hide()
                event.prevent_default()
                event.stop()
                return

        if event.key == "enter":
            text = self.text.strip()
            if text:
                self.post_message(self.Submitted(text))
                self.clear()
            event.prevent_default()
            event.stop()
            return

    def _accept_completion(self, completer: ChatCompleter, add_space: bool) -> None:
        sel = completer.selected
        if sel is None:
            return
        text = self.text
        trigger_start = completer.trigger_start

        if completer.trigger_char == "/":
            insert = sel + (" " if add_space else "")
        elif completer.trigger_char == "@":
            if add_space and not sel.endswith("/"):
                insert = sel + " "
            else:
                insert = sel
        else:
            insert = sel

        new_text = text[: trigger_start + 1] + insert
        self.load_text(new_text)
        if not add_space:
            cursor_pos = len(new_text)
            completer.update_completions(new_text, cursor_pos)


class ChatPanel(Static):
    DEFAULT_CSS = "ChatPanel { width: 1fr; }"

    def __init__(self, **kwargs) -> None:
        super().__init__(id="panel-chat", **kwargs)

    def compose(self) -> ComposeResult:
        yield Label("Chat", classes="panel-title")
        yield Vertical(id="chat-messages")
        yield ChatCompleter(id="chat-completer")
        yield ChatInput(id="chat-input")


class AgentPanel(Static):
    DEFAULT_CSS = "AgentPanel { width: 16; }"

    def __init__(self, **kwargs) -> None:
        super().__init__(id="panel-agent", **kwargs)

    class EditAgent(Message):
        def __init__(self, agent_name: str) -> None:
            super().__init__()
            self.agent_name = agent_name

    def compose(self) -> ComposeResult:
        yield Label("Agents", classes="panel-title")
        yield ListView(id="agent-list")

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Double-click / Enter on agent triggers edit."""
        item = event.item
        name = getattr(item, "data", None)
        if name is not None:
            self.post_message(self.EditAgent(name))


class ResizeHandle(Static):
    """Draggable vertical divider between panels."""

    DEFAULT_CSS = """
    ResizeHandle {
        width: 1;
        height: 100%;
        background: $panel;
        color: $foreground 50%;
    }
    ResizeHandle:hover {
        background: $accent;
        color: $foreground;
    }
    ResizeHandle.dragging {
        background: $accent;
        color: $foreground;
    }
    """

    def __init__(self, left_panel_id: str, right_panel_id: str, **kwargs) -> None:
        super().__init__("│", **kwargs)
        self._left_id = left_panel_id
        self._right_id = right_panel_id
        self._dragging = False
        self._start_x = 0
        self._left_start_width = 0

    def on_mouse_down(self, event: MouseDown) -> None:
        self._dragging = True
        self._start_x = event.screen_x
        left = self.screen.query_one(f"#{self._left_id}")
        self._left_start_width = left.size.width
        self.add_class("dragging")
        self.capture_mouse()
        event.stop()

    def on_mouse_move(self, event: MouseMove) -> None:
        if not self._dragging:
            return
        delta = event.screen_x - self._start_x
        new_width = max(8, self._left_start_width + delta)
        left = self.screen.query_one(f"#{self._left_id}")
        left.styles.width = new_width
        event.stop()

    def on_mouse_up(self, event: MouseUp) -> None:
        if self._dragging:
            self._dragging = False
            self.remove_class("dragging")
            self.release_mouse()
            event.stop()


class HumuApp(App):
    CSS = """
    Screen {
        layout: horizontal;
    }
    .panel-title {
        text-style: bold;
        background: $surface;
        width: 100%;
        padding: 0 1;
    }
    WorkspacePanel:focus-within > .panel-title,
    RoomPanel:focus-within > .panel-title,
    ChatPanel:focus-within > .panel-title,
    AgentPanel:focus-within > .panel-title {
        background: $primary;
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

    BINDINGS = [
        ("ctrl+n", "create_new", "New"),
        ("ctrl+d", "delete_selected", "Delete"),
    ]

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
            yield ResizeHandle("panel-workspace", "panel-room")
            yield RoomPanel()
            yield ResizeHandle("panel-room", "panel-chat")
            yield ChatPanel()
            yield ResizeHandle("panel-chat", "panel-agent")
            yield AgentPanel()
        yield Footer()

    def _load_client_state(self) -> dict:
        try:
            return json.loads(CLIENT_STATE.read_text())
        except Exception:
            return {}

    def _save_client_state(self) -> None:
        state = {}
        if self._current_workspace:
            state["workspace"] = self._current_workspace
        if self._current_room:
            state["room"] = self._current_room
        try:
            CLIENT_STATE.parent.mkdir(parents=True, exist_ok=True)
            CLIENT_STATE.write_text(json.dumps(state))
        except Exception:
            logger.exception("Failed to save client state")

    async def on_mount(self) -> None:
        try:
            await self._http.start()
        except Exception:
            logger.exception("Failed to start HTTP client")
            return
        try:
            await self._conn.connect()
        except Exception:
            logger.exception("WebSocket connection failed")
            return
        self.run_worker(self._conn.receive_loop(), exclusive=True, group="ws")
        await self._load_workspaces()
        await self._restore_selection()

    async def on_unmount(self) -> None:
        await self._conn.disconnect()
        await self._http.stop()

    async def _restore_selection(self) -> None:
        state = self._load_client_state()
        ws_name = state.get("workspace")
        room_name = state.get("room")
        if not ws_name:
            return

        # Select workspace in list
        lv = self.query_one("#workspace-list", ListView)
        for i, child in enumerate(lv.children):
            if getattr(child, "data", None) == ws_name:
                lv.index = i
                self._current_workspace = ws_name
                await self._load_rooms(ws_name)
                break
        else:
            return

        if not room_name:
            return

        # Select room in list
        rlv = self.query_one("#room-list", ListView)
        for i, child in enumerate(rlv.children):
            if getattr(child, "data", None) == room_name:
                rlv.index = i
                self._current_room = room_name
                await self._load_agents(ws_name, room_name)
                await self._conn.subscribe_room(ws_name, room_name)
                break

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
            # Update chat completer workspace root
            try:
                ws_data = next(
                    ws for ws in await self._http.list_workspaces()
                    if ws["name"] == name
                )
                completer = self.query_one("#chat-completer", ChatCompleter)
                completer.set_workspace_root(ws_data.get("root_path"))
            except Exception:
                pass
            self._save_client_state()

        elif list_id == "room-list" and self._current_workspace:
            self._current_room = name
            await self._load_agents(self._current_workspace, name)
            await self._conn.subscribe_room(self._current_workspace, name)
            self._save_client_state()

        # agent-list selection handled by AgentPanel.EditAgent message

    # --- Create Flow (Ctrl+N) ---

    def action_create_new(self) -> None:
        focused = self.focused
        if focused is None:
            return

        node = focused
        while node is not None:
            if isinstance(node, WorkspacePanel):
                self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)
                return
            if isinstance(node, RoomPanel):
                if self._current_workspace is None:
                    self.notify("Select a workspace first", severity="warning")
                    return
                self.push_screen(CreateRoomScreen(), self._on_room_created)
                return
            if isinstance(node, AgentPanel):
                if self._current_workspace is None or self._current_room is None:
                    self.notify("Select a room first", severity="warning")
                    return
                self.push_screen(CreateAgentScreen(), self._on_agent_created)
                return
            node = node.parent

    async def _on_workspace_created(self, result: dict | None) -> None:
        if result is None:
            return
        try:
            await self._http.create_workspace(result["name"], result["root_path"])
        except Exception as e:
            self.notify(f"Failed to create workspace: {e}", severity="error")
            return
        await self._load_workspaces()

    async def _on_room_created(self, result: str | None) -> None:
        if result is None or self._current_workspace is None:
            return
        try:
            await self._http.create_room(self._current_workspace, result)
        except Exception as e:
            self.notify(f"Failed to create room: {e}", severity="error")
            return
        await self._load_rooms(self._current_workspace)

    async def _on_agent_created(self, result: dict | None) -> None:
        if result is None or self._current_workspace is None or self._current_room is None:
            return
        try:
            await self._http.create_agent(
                self._current_workspace, self._current_room, result
            )
        except Exception as e:
            self.notify(f"Failed to create agent: {e}", severity="error")
            return
        await self._load_agents(self._current_workspace, self._current_room)

    # --- Delete Flow (Ctrl+D) ---

    def action_delete_selected(self) -> None:
        focused = self.focused
        if focused is None:
            return

        node = focused
        while node is not None:
            if isinstance(node, WorkspacePanel):
                self._delete_workspace()
                return
            if isinstance(node, RoomPanel):
                self._delete_room()
                return
            if isinstance(node, AgentPanel):
                self._delete_agent()
                return
            node = node.parent

    def _delete_workspace(self) -> None:
        lv = self.query_one("#workspace-list", ListView)
        if lv.index is None:
            return
        item = lv.children[lv.index]
        name = getattr(item, "data", None)
        if name is None:
            return
        self.push_screen(
            ConfirmDeleteScreen(
                f"Delete workspace '{name}'? This will remove all rooms, agents, "
                "and chat history. This cannot be undone."
            ),
            lambda confirmed: self._do_delete_workspace(name) if confirmed else None,
        )

    async def _do_delete_workspace(self, name: str) -> None:
        try:
            await self._http.delete_workspace(name)
        except Exception as e:
            self.notify(f"Failed to delete workspace: {e}", severity="error")
            return
        self._current_workspace = None
        self._current_room = None
        await self._clear_rooms()
        await self._clear_agents()
        await self._clear_chat()
        await self._load_workspaces()

    def _delete_room(self) -> None:
        if self._current_workspace is None:
            return
        lv = self.query_one("#room-list", ListView)
        if lv.index is None:
            return
        item = lv.children[lv.index]
        name = getattr(item, "data", None)
        if name is None:
            return
        self.push_screen(
            ConfirmDeleteScreen(
                f"Delete room '{name}'? Chat history will be lost. This cannot be undone."
            ),
            lambda confirmed, n=name: self._do_delete_room(n) if confirmed else None,
        )

    async def _do_delete_room(self, name: str) -> None:
        try:
            await self._http.delete_room(self._current_workspace, name)
        except Exception as e:
            self.notify(f"Failed to delete room: {e}", severity="error")
            return
        self._current_room = None
        await self._clear_agents()
        await self._clear_chat()
        await self._load_rooms(self._current_workspace)

    def _delete_agent(self) -> None:
        if self._current_workspace is None or self._current_room is None:
            return
        lv = self.query_one("#agent-list", ListView)
        if lv.index is None:
            return
        item = lv.children[lv.index]
        name = getattr(item, "data", None)
        if name is None:
            return
        self.push_screen(
            ConfirmDeleteScreen(
                f"Delete agent '{name}'? This cannot be undone."
            ),
            lambda confirmed, n=name: self._do_delete_agent(n) if confirmed else None,
        )

    async def _do_delete_agent(self, name: str) -> None:
        try:
            await self._http.delete_agent(
                self._current_workspace, self._current_room, name
            )
        except Exception as e:
            self.notify(f"Failed to delete agent: {e}", severity="error")
            return
        await self._load_agents(self._current_workspace, self._current_room)

    # --- Agent Edit (double-click / Enter in agent list) ---

    async def on_agent_panel_edit_agent(self, event: AgentPanel.EditAgent) -> None:
        if self._current_workspace is None or self._current_room is None:
            return
        try:
            agents = await self._http.list_agents(
                self._current_workspace, self._current_room
            )
        except Exception:
            return
        agent_data = None
        for a in agents:
            if a["name"] == event.agent_name:
                agent_data = a
                break
        if agent_data is None:
            return
        self.push_screen(
            CreateAgentScreen(agent_data=agent_data), self._on_agent_edited
        )

    async def _on_agent_edited(self, result: dict | None) -> None:
        if result is None or self._current_workspace is None or self._current_room is None:
            return
        try:
            await self._http.update_agent(
                self._current_workspace,
                self._current_room,
                result["name"],
                result,
            )
        except Exception as e:
            self.notify(f"Failed to update agent: {e}", severity="error")
            return
        await self._load_agents(self._current_workspace, self._current_room)

    # --- Chat Input ---

    async def on_chat_input_submitted(self, event: ChatInput.Submitted) -> None:
        if self._current_workspace and self._current_room:
            await self._conn.send_message(
                self._current_workspace, self._current_room, event.text
            )

    def on_text_area_changed(self, event: TextArea.Changed) -> None:
        if event.text_area.id == "chat-input":
            completer = self.query_one("#chat-completer", ChatCompleter)
            text = event.text_area.text
            cursor_pos = len(text)
            completer.update_completions(text, cursor_pos)

    # --- Server Events ---

    def _handle_server_event(self, event: dict) -> None:
        event_type = event.get("type", "")
        if event_type == "room_state_sync":
            self.call_later(self._render_history, event.get("messages", []))
        elif event_type == "message_added":
            self.call_later(
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
    from humu.config import HUMU_HOME

    HUMU_HOME.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(
        filename=str(HUMU_HOME / "client.log"),
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    app = HumuApp()
    app.run()
