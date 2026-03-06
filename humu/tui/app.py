from __future__ import annotations

import asyncio

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.widgets import Footer, Header, Input

from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.services.agent_runner import AgentRunner
from humu.services.router import Router
from humu.services.storage import Storage
from humu.tui.screens.create_agent import CreateAgentScreen
from humu.tui.screens.create_room import CreateRoomScreen, RoomCreateResult
from humu.tui.screens.create_workspace import CreateWorkspaceScreen
from humu.tui.widgets.agent_panel import AgentPanel
from humu.tui.widgets.chat_panel import ChatPanel, MessageSubmitted
from humu.tui.widgets.room_panel import RoomNewRequested, RoomPanel, RoomSelected
from humu.tui.widgets.workspace_panel import (
    WorkspaceNewRequested,
    WorkspacePanel,
    WorkspaceSelected,
)

RELOAD_EXIT_CODE = "reload"


class HumuApp(App):
    CSS = """
    #main-layout {
        height: 1fr;
    }
    """

    TITLE = "Humu"
    BINDINGS = [
        Binding("ctrl+n", "create_new", "New", show=True),
        Binding("ctrl+d", "delete_selected", "Delete", show=True),
        Binding("ctrl+r", "restart", "Reload", show=True),
        Binding("ctrl+c", "quit_or_warn", "Quit (x2)", show=True),
        Binding("tab", "focus_next", "Next Panel", show=False),
        Binding("shift+tab", "focus_previous", "Prev Panel", show=False),
    ]

    def __init__(self) -> None:
        super().__init__()
        self._storage = Storage()
        self._runner = AgentRunner(self._storage)
        self._router = Router(self._runner, self._storage)
        self._current_workspace: Workspace | None = None
        self._current_room: Room | None = None
        self._processing = False
        self._quit_pending = False
        self._quit_timer: object | None = None

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="main-layout"):
            yield WorkspacePanel()
            yield RoomPanel()
            yield ChatPanel()
            yield AgentPanel()
        yield Footer()

    def on_mount(self) -> None:
        self._refresh_workspaces()
        self._restore_last_session()

    def _restore_last_session(self) -> None:
        last = self._storage.load_last_session()
        if not last:
            return
        workspace_name, room_name = last
        ws = self._storage.get_workspace(workspace_name)
        if not ws:
            return
        room = self._storage.get_room(ws, room_name)
        if not room:
            return
        self._current_workspace = ws
        self._current_room = room
        self._refresh_workspaces()
        self._refresh_rooms()
        self._refresh_agents()
        self._refresh_chat()

    # --- Refresh UI ---

    def _refresh_workspaces(self) -> None:
        workspaces = self._storage.list_workspaces()
        names = [w.name for w in workspaces]
        selected = self._current_workspace.name if self._current_workspace else None
        self.query_one(WorkspacePanel).set_workspaces(names, selected)

    def _refresh_rooms(self) -> None:
        room_panel = self.query_one(RoomPanel)
        if not self._current_workspace:
            room_panel.set_rooms([])
            return
        rooms = self._storage.list_rooms(self._current_workspace)
        names = [r.name for r in rooms]
        selected = self._current_room.name if self._current_room else None
        room_panel.set_rooms(names, selected)

    def _refresh_agents(self) -> None:
        agent_panel = self.query_one(AgentPanel)
        if not self._current_room:
            agent_panel.set_agents(None)
            return
        agent_panel.set_agents(
            self._current_room.leader, self._current_room.agents
        )

    def _refresh_chat(self) -> None:
        chat_panel = self.query_one(ChatPanel)
        if not self._current_workspace or not self._current_room:
            chat_panel.set_workspace_path(None)
            chat_panel.set_room(None)
            chat_panel.clear_messages()
            return
        chat_panel.set_workspace_path(self._current_workspace.root_path)
        chat_panel.set_room(self._current_room.name)
        history = self._storage.load_chat_history(
            self._current_workspace, self._current_room.name
        )
        chat_panel.load_history(history)

    # --- Event handlers ---

    def on_workspace_new_requested(self, event: WorkspaceNewRequested) -> None:
        self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)

    def on_workspace_selected(self, event: WorkspaceSelected) -> None:
        ws = self._storage.get_workspace(event.name)
        if ws:
            self._current_workspace = ws
            self._current_room = None
            self._refresh_workspaces()
            self._refresh_rooms()
            self._refresh_agents()
            self._refresh_chat()

    def on_room_new_requested(self, event: RoomNewRequested) -> None:
        if not self._current_workspace:
            self.notify("Select a workspace first.", severity="warning")
            return
        self.push_screen(CreateRoomScreen(), self._on_room_created)

    def on_room_selected(self, event: RoomSelected) -> None:
        if not self._current_workspace:
            return
        room = self._storage.get_room(self._current_workspace, event.name)
        if room:
            self._current_room = room
            self._refresh_rooms()
            self._refresh_agents()
            self._refresh_chat()
            self._storage.save_last_session(self._current_workspace.name, room.name)

    async def on_message_submitted(self, event: MessageSubmitted) -> None:
        text = event.text

        if text.startswith("/"):
            await self._handle_command(text)
            return

        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return

        if self._processing:
            self.notify("Processing previous message...", severity="warning")
            return

        self._processing = True
        chat = self.query_one(ChatPanel)

        chat.add_message("you", text)
        self._storage.append_chat_message(
            self._current_workspace,
            self._current_room.name,
            {"sender": "you", "text": text},
        )

        workspace = self._current_workspace
        room = self._current_room

        def _process_sync() -> None:
            loop = asyncio.new_event_loop()
            try:
                loop.run_until_complete(self._process_message(workspace, room, text, chat))
            finally:
                loop.close()

        self.run_worker(_process_sync, thread=True)

    async def _process_message(
        self,
        workspace: Workspace,
        room: Room,
        text: str,
        chat: ChatPanel,
    ) -> None:
        try:
            async for msg in self._router.handle_message(workspace, room, text):
                if msg.is_loading:
                    self.call_from_thread(chat.show_loading, msg.sender)
                    continue
                self.call_from_thread(chat.hide_loading)
                self.call_from_thread(
                    chat.add_message, msg.sender, msg.text, msg.is_system, msg.raw, msg.steps,
                )
                self._storage.append_chat_message(
                    workspace,
                    room.name,
                    {
                        "sender": msg.sender,
                        "text": msg.text,
                        "is_system": msg.is_system,
                        "raw": msg.raw,
                        "steps": msg.steps,
                    },
                )
        except Exception as e:
            import traceback
            err_detail = f"{e}\n{traceback.format_exc()}"
            self.call_from_thread(chat.add_message, "error", err_detail, True)
        finally:
            self.call_from_thread(chat.hide_loading)
            self._processing = False

    # --- Commands ---

    async def _handle_command(self, text: str) -> None:
        parts = text.strip().split(maxsplit=2)
        cmd = parts[0].lower()

        if cmd == "/invite":
            await self._cmd_invite(parts)
        elif cmd == "/kick":
            await self._cmd_kick(parts)
        elif cmd == "/agents":
            self._cmd_list_agents()
        elif cmd == "/rooms":
            self._cmd_list_rooms()
        elif cmd == "/status":
            self._cmd_status()
        elif cmd == "/help":
            self._cmd_help()
        else:
            self.notify(f"Unknown command: {cmd}", severity="error")

    async def _cmd_invite(self, parts: list[str]) -> None:
        if len(parts) < 2:
            self.notify("Usage: /invite <agent-name>", severity="error")
            return
        agent_name = parts[1]
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return
        agent = self._storage.get_agent(agent_name)
        if not agent:
            self.notify(f"Agent '{agent_name}' not found.", severity="error")
            return
        if agent_name in self._current_room.agents:
            self.notify(f"Agent '{agent_name}' already in room.", severity="warning")
            return
        if agent_name == self._current_room.leader:
            self.notify(f"Agent '{agent_name}' is already the leader.", severity="warning")
            return
        self._current_room.agents.append(agent_name)
        self._storage.save_room(self._current_workspace, self._current_room)
        self._refresh_agents()
        chat = self.query_one(ChatPanel)
        chat.add_message("system", f"Invited {agent_name} to the room.", is_system=True)

    async def _cmd_kick(self, parts: list[str]) -> None:
        if len(parts) < 2:
            self.notify("Usage: /kick <agent-name>", severity="error")
            return
        agent_name = parts[1]
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return
        if agent_name == self._current_room.leader:
            self.notify("Cannot kick the leader agent.", severity="error")
            return
        if agent_name not in self._current_room.agents:
            self.notify(f"Agent '{agent_name}' is not in this room.", severity="warning")
            return
        self._current_room.agents.remove(agent_name)
        self._storage.save_room(self._current_workspace, self._current_room)
        self._refresh_agents()
        chat = self.query_one(ChatPanel)
        chat.add_message("system", f"Kicked {agent_name} from the room.", is_system=True)

    def _cmd_list_agents(self) -> None:
        agents = self._storage.list_agents()
        if agents:
            lines = [f"  {a.name} - {a.description}" for a in agents]
            self.notify("Agents:\n" + "\n".join(lines))
        else:
            self.notify("No agents defined.")

    def _cmd_list_rooms(self) -> None:
        if not self._current_workspace:
            self.notify("Select a workspace first.", severity="warning")
            return
        rooms = self._storage.list_rooms(self._current_workspace)
        if rooms:
            lines = [f"  {r.name} (leader: {r.leader})" for r in rooms]
            self.notify("Rooms:\n" + "\n".join(lines))
        else:
            self.notify("No rooms in this workspace.")

    def _cmd_status(self) -> None:
        ws = self._current_workspace
        room = self._current_room
        parts = []
        parts.append(f"Workspace: {ws.name if ws else 'None'}")
        if ws:
            parts.append(f"Path: {ws.root_path}")
        parts.append(f"Room: {room.name if room else 'None'}")
        if room:
            parts.append(f"Leader: {room.leader}")
            parts.append(f"Agents: {', '.join(room.agents) or 'None'}")
        self.notify("\n".join(parts))

    def _cmd_help(self) -> None:
        self.notify(
            "Commands:\n"
            "  /invite <agent>  — Invite agent to room\n"
            "  /kick <agent>    — Remove agent from room\n"
            "  /agents          — List all agents\n"
            "  /rooms           — List rooms in workspace\n"
            "  /status          — Show current state\n"
            "  /help            — Show this help\n"
            "\n"
            "Keys:\n"
            "  Ctrl+N — Create new item\n"
            "  Ctrl+D — Delete selected item"
        )

    # --- Actions ---

    def action_create_new(self) -> None:
        focused = self.focused
        if focused is None:
            self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)
            return

        # Walk up the DOM to find which panel the focused widget is in
        node = focused
        while node is not None:
            if isinstance(node, WorkspacePanel):
                self.push_screen(
                    CreateWorkspaceScreen(), self._on_workspace_created
                )
                return
            if isinstance(node, RoomPanel):
                if not self._current_workspace:
                    self.notify("Select a workspace first.", severity="warning")
                    return
                self.push_screen(CreateRoomScreen(), self._on_room_created)
                return
            if isinstance(node, AgentPanel):
                self.push_screen(
                    CreateAgentScreen(), self._on_agent_created
                )
                return
            if isinstance(node, ChatPanel):
                if not self._current_workspace:
                    self.notify("Select a workspace first.", severity="warning")
                    return
                self.push_screen(CreateRoomScreen(), self._on_room_created)
                return
            node = node.parent

        # Fallback: create workspace
        self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)

    def _on_workspace_created(self, result: Workspace | None) -> None:
        if result:
            self._storage.save_workspace(result)
            self._current_workspace = result
            self._current_room = None
            self._refresh_workspaces()
            self._refresh_rooms()
            self._refresh_agents()
            self._refresh_chat()
            self.notify(f"Workspace '{result.name}' created.")

    def _on_room_created(self, result: RoomCreateResult | None) -> None:
        if result and self._current_workspace:
            leader_name = f"{result.name}-leader"
            if not self._storage.get_agent(leader_name):
                from humu.models.agent import AgentConfig

                leader = AgentConfig(
                    name=leader_name,
                    description=f"Leader agent for room '{result.name}'. Routes messages to the right agents.",
                    prompt=(
                        "You are a helpful leader agent. "
                        "Read user messages and decide whether to answer directly "
                        "or forward to a specialist agent in the room."
                    ),
                )
                self._storage.save_agent(leader)
            room = Room(name=result.name, leader=leader_name)
            self._storage.save_room(self._current_workspace, room)
            self._current_room = room
            self._refresh_rooms()
            self._refresh_agents()
            self._refresh_chat()
            self._storage.save_last_session(self._current_workspace.name, room.name)
            self.notify(f"Room '{result.name}' created with leader '{leader_name}'.")

    def _on_agent_created(self, result: object) -> None:
        from humu.models.agent import AgentConfig

        if isinstance(result, AgentConfig):
            self._storage.save_agent(result)
            self._refresh_agents()
            self.notify(f"Agent '{result.name}' created.")

    def action_delete_selected(self) -> None:
        focused = self.focused
        if focused is None:
            return

        node = focused
        while node is not None:
            if isinstance(node, WorkspacePanel):
                if self._current_workspace:
                    name = self._current_workspace.name
                    self._storage.delete_workspace(name)
                    self._current_workspace = None
                    self._current_room = None
                    self._refresh_workspaces()
                    self._refresh_rooms()
                    self._refresh_agents()
                    self._refresh_chat()
                    self.notify(f"Workspace '{name}' deleted.")
                return
            if isinstance(node, RoomPanel):
                if self._current_workspace and self._current_room:
                    name = self._current_room.name
                    self._storage.delete_room(self._current_workspace, name)
                    self._current_room = None
                    self._refresh_rooms()
                    self._refresh_agents()
                    self._refresh_chat()
                    self.notify(f"Room '{name}' deleted.")
                return
            node = node.parent

    def action_quit_or_warn(self) -> None:
        if self._quit_pending:
            self.exit()
            return

        # First Ctrl+C: clear chat input text
        chat_input = self.query_one("#chat-input", Input)
        if chat_input.value:
            chat_input.value = ""
        else:
            self._quit_pending = True
            self.notify("Press Ctrl+C again to quit.", timeout=2)
            self._quit_timer = self.set_timer(2, self._reset_quit)

    def _reset_quit(self) -> None:
        self._quit_pending = False

    def action_restart(self) -> None:
        self.exit(result=RELOAD_EXIT_CODE)
