from __future__ import annotations

import asyncio

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.widgets import Footer, Header

from humu.client.connection import Connection
from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.services.storage import Storage
from humu.tui.screens.create_agent import CreateAgentScreen
from humu.tui.screens.create_room import CreateRoomScreen, RoomCreateResult
from humu.tui.screens.create_workspace import CreateWorkspaceScreen
from humu.tui.widgets.agent_panel import AgentEditRequested, AgentPanel
from humu.tui.widgets.chat_panel import ChatPanel, MessageSubmitted
from humu.tui.widgets.resize_handle import ResizeHandle
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
        Binding("ctrl+m", "plugin_manager", "Plugins", show=True),
        Binding("ctrl+c", "quit_or_warn", "Quit (x2)", show=True),
        Binding("escape", "cancel_processing", "Cancel", show=False),
        Binding("tab", "focus_next", "Next Panel", show=False),
        Binding("shift+tab", "focus_previous", "Prev Panel", show=False),
    ]

    def __init__(self) -> None:
        super().__init__()
        # Local storage for client-only state (theme, panel widths, last session)
        self._local_storage = Storage()
        # Server connection
        self._conn = Connection()
        self._conn.on_event = self._on_server_event

        self._current_workspace: Workspace | None = None
        self._current_room: Room | None = None
        # Set of (workspace_name, room_name) currently being processed
        self._processing: set[tuple[str, str]] = set()
        self._quit_pending = False
        self._quit_timer: object | None = None
        # Maps (workspace_name, room_name) -> sender for active loading indicators
        self._active_loading: dict[tuple[str, str], str] = {}
        # Spinner animation for processing indicators
        self._spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        self._spinner_frame: int = 0
        self._spinner_timer: object | None = None
        # Cached skills for autocomplete
        self._cached_skills: list[dict] = []

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="main-layout"):
            yield WorkspacePanel(id="workspace-panel")
            yield ResizeHandle("workspace-panel", min_width=10, save_callback=self._save_panel_width)
            yield RoomPanel(id="room-panel")
            yield ResizeHandle("room-panel", min_width=8, save_callback=self._save_panel_width)
            yield ChatPanel(get_skills=lambda: self._cached_skills)
            yield ResizeHandle("agent-panel", min_width=10, invert=True, save_callback=self._save_panel_width)
            yield AgentPanel(id="agent-panel")
        yield Footer()

    def on_mount(self) -> None:
        saved_theme = self._local_storage.load_theme()
        if saved_theme:
            self.theme = saved_theme
        self.query_one(Header).icon = "Menu"
        self._restore_panel_widths()
        # Start connection in background
        self.run_worker(self._connect_and_restore, thread=False)

    async def _connect_and_restore(self) -> None:
        """Connect to server and restore UI state."""
        await self._conn.connect()

        # Fetch skills for autocomplete
        reply = await self._conn.send({"type": "get_skills"})
        if reply and reply.get("type") == "skills_list":
            self._cached_skills = reply["skills"]

        # Fetch processing state
        reply = await self._conn.send({"type": "get_processing_state"})
        if reply and reply.get("type") == "processing_state":
            for r in reply.get("rooms", []):
                self._processing.add((r["workspace"], r["room"]))
            if self._processing:
                self._start_spinner()

        # Restore last session
        last = self._local_storage.load_last_session()
        if last:
            ws_name, room_name = last
            await self._select_workspace(ws_name)
            await self._select_room(room_name)
        else:
            await self._refresh_workspaces()

    # ------------------------------------------------------------------
    # Server event handling
    # ------------------------------------------------------------------

    def _on_server_event(self, event: dict) -> None:
        """Called from the read loop when a broadcast event arrives."""
        self.call_from_thread(self._dispatch_event, event)

    def _dispatch_event(self, event: dict) -> None:
        etype = event.get("type", "")
        ws = event.get("workspace", "")
        room = event.get("room", "")

        if etype == "message_added":
            self._on_message_added(event)
        elif etype == "processing_started":
            room_key = (ws, room)
            self._processing.add(room_key)
            self._active_loading[room_key] = event.get("sender", "")
            self._start_spinner()
            if self._is_viewing_key(ws, room):
                chat = self.query_one(ChatPanel)
                chat.show_loading(event.get("sender", ""))
        elif etype == "processing_done":
            room_key = (ws, room)
            self._processing.discard(room_key)
            self._active_loading.pop(room_key, None)
            if not self._processing:
                self._stop_spinner()
            else:
                self._refresh_workspaces_sync()
                self._refresh_rooms_sync()
            if self._is_viewing_key(ws, room):
                self.query_one(ChatPanel).hide_loading()
                self._refresh_agents_sync()
        elif etype == "processing_cancelled":
            room_key = (ws, room)
            self._processing.discard(room_key)
            self._active_loading.pop(room_key, None)
            if not self._processing:
                self._stop_spinner()
            if self._is_viewing_key(ws, room):
                self.query_one(ChatPanel).hide_loading()
        elif etype == "queue_updated":
            if self._is_viewing_key(ws, room):
                self.query_one(ChatPanel).set_pending_queue(event.get("pending_messages", []))
        elif etype == "workspace_list":
            self._apply_workspace_list(event["workspaces"])
        elif etype == "room_list":
            if ws == (self._current_workspace.name if self._current_workspace else ""):
                self._apply_room_list(event["rooms"])
        elif etype == "agent_list":
            self._refresh_agents_sync()
        elif etype == "chat_history":
            if self._is_viewing_key(ws, room):
                chat = self.query_one(ChatPanel)
                chat.load_history(event["messages"])
        elif etype == "system_event":
            if self._is_viewing_key(ws, room):
                chat = self.query_one(ChatPanel)
                chat.add_message(event.get("agent", "system"), event.get("text", ""))

    def _on_message_added(self, event: dict) -> None:
        ws = event.get("workspace", "")
        room = event.get("room", "")
        if not self._is_viewing_key(ws, room):
            return
        chat = self.query_one(ChatPanel)
        chat.hide_loading()
        chat.add_message(
            event["sender"],
            event["text"],
            event.get("is_system", False),
            event.get("raw"),
            event.get("steps", []),
            context_pct=event.get("context_pct"),
        )

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _is_viewing_key(self, ws: str, room: str) -> bool:
        return (
            self._current_workspace is not None
            and self._current_room is not None
            and self._current_workspace.name == ws
            and self._current_room.name == room
        )

    def _is_viewing(self, workspace: Workspace, room: Room) -> bool:
        return self._is_viewing_key(workspace.name, room.name)

    # ------------------------------------------------------------------
    # Server communication helpers (async, run from workers)
    # ------------------------------------------------------------------

    async def _server_send(self, msg: dict) -> dict | None:
        return await self._conn.send(msg)

    async def _select_workspace(self, name: str) -> None:
        reply = await self._conn.send({"type": "list_workspaces"})
        if reply and reply.get("type") == "workspace_list":
            self._apply_workspace_list(reply["workspaces"])
        for w in (reply or {}).get("workspaces", []):
            if w["name"] == name:
                self._current_workspace = Workspace.from_dict(w)
                break
        if self._current_workspace:
            await self._refresh_rooms()

    async def _select_room(self, name: str) -> None:
        if not self._current_workspace:
            return
        reply = await self._conn.send({"type": "list_rooms", "workspace": self._current_workspace.name})
        if reply and reply.get("type") == "room_list":
            self._apply_room_list(reply["rooms"])
        for r in (reply or {}).get("rooms", []):
            if r["name"] == name:
                self._current_room = Room.from_dict(r)
                break
        if self._current_room:
            self._refresh_agents_sync()
            await self._refresh_chat()
            self._subscribe_current_room()

    def _subscribe_current_room(self) -> None:
        if self._current_workspace and self._current_room:
            asyncio.ensure_future(
                self._conn.send_nowait({
                    "type": "subscribe_room",
                    "workspace": self._current_workspace.name,
                    "room": self._current_room.name,
                })
            )

    def _unsubscribe_room(self, ws_name: str, room_name: str) -> None:
        asyncio.ensure_future(
            self._conn.send_nowait({
                "type": "unsubscribe_room",
                "workspace": ws_name,
                "room": room_name,
            })
        )

    # ------------------------------------------------------------------
    # Refresh UI (sync versions for call_from_thread)
    # ------------------------------------------------------------------

    def _apply_workspace_list(self, workspaces: list[dict]) -> None:
        names = [w["name"] for w in workspaces]
        selected = self._current_workspace.name if self._current_workspace else None
        processing_ws = {ws for ws, _ in self._processing}
        spinner = self._spinner_frames[self._spinner_frame]
        self.query_one(WorkspacePanel).set_workspaces(names, selected, processing_ws, spinner)

    def _apply_room_list(self, rooms: list[dict]) -> None:
        names = [r["name"] for r in rooms]
        selected = self._current_room.name if self._current_room else None
        ws_name = self._current_workspace.name if self._current_workspace else ""
        processing_rooms = {room for ws, room in self._processing if ws == ws_name}
        spinner = self._spinner_frames[self._spinner_frame]
        self.query_one(RoomPanel).set_rooms(names, selected, processing_rooms, spinner)

    def _refresh_workspaces_sync(self) -> None:
        """Refresh workspace panel using cached processing state."""
        # We need the workspace list — request it async
        asyncio.ensure_future(self._refresh_workspaces())

    def _refresh_rooms_sync(self) -> None:
        asyncio.ensure_future(self._refresh_rooms())

    def _refresh_agents_sync(self) -> None:
        asyncio.ensure_future(self._refresh_agents())

    async def _refresh_workspaces(self) -> None:
        reply = await self._conn.send({"type": "list_workspaces"})
        if reply and reply.get("type") == "workspace_list":
            self._apply_workspace_list(reply["workspaces"])

    async def _refresh_rooms(self) -> None:
        if not self._current_workspace:
            self.query_one(RoomPanel).set_rooms([])
            return
        reply = await self._conn.send({"type": "list_rooms", "workspace": self._current_workspace.name})
        if reply and reply.get("type") == "room_list":
            self._apply_room_list(reply["rooms"])

    async def _refresh_agents(self) -> None:
        agent_panel = self.query_one(AgentPanel)
        if not self._current_room:
            agent_panel.set_agents(None)
            return
        all_agent_names = [self._current_room.leader] + list(self._current_room.agents)
        agent_models: dict[str, str] = {}
        for aname in all_agent_names:
            reply = await self._conn.send({"type": "get_agent", "name": aname})
            if reply and reply.get("type") == "agent_info" and reply.get("agent"):
                agent_models[aname] = reply["agent"].get("model", "")
        agent_panel.set_agents(
            self._current_room.leader, self._current_room.agents, agent_models
        )

    async def _refresh_chat(self) -> None:
        chat_panel = self.query_one(ChatPanel)
        if not self._current_workspace or not self._current_room:
            chat_panel.set_workspace_path(None)
            chat_panel.set_room(None)
            chat_panel.clear_messages()
            return
        chat_panel.set_workspace_path(self._current_workspace.root_path)
        chat_panel.set_room(self._current_room.name)
        reply = await self._conn.send({
            "type": "get_chat_history",
            "workspace": self._current_workspace.name,
            "room": self._current_room.name,
        })
        if reply and reply.get("type") == "chat_history":
            chat_panel.load_history(reply["messages"])
        # Re-show loading if room is processing
        room_key = (self._current_workspace.name, self._current_room.name)
        sender = self._active_loading.get(room_key)
        if sender:
            chat_panel.show_loading(sender)
        # Update queue display
        chat_panel.set_pending_queue([])

    # ------------------------------------------------------------------
    # Spinner
    # ------------------------------------------------------------------

    def _start_spinner(self) -> None:
        if self._spinner_timer is None:
            self._spinner_timer = self.set_interval(0.1, self._tick_spinner)
        self._refresh_workspaces_sync()
        self._refresh_rooms_sync()

    def _stop_spinner(self) -> None:
        if self._spinner_timer is not None:
            self._spinner_timer.stop()
            self._spinner_timer = None
        self._refresh_workspaces_sync()
        self._refresh_rooms_sync()

    def _tick_spinner(self) -> None:
        self._spinner_frame = (self._spinner_frame + 1) % len(self._spinner_frames)
        spinner = self._spinner_frames[self._spinner_frame]
        processing_ws = {ws for ws, _ in self._processing}
        ws_name = self._current_workspace.name if self._current_workspace else None
        processing_rooms = (
            {room for ws, room in self._processing if ws == ws_name} if ws_name else set()
        )
        self.query_one(WorkspacePanel).update_spinner(
            processing_ws, spinner, ws_name
        )
        self.query_one(RoomPanel).update_spinner(
            processing_rooms, spinner,
            self._current_room.name if self._current_room else None,
        )

    # ------------------------------------------------------------------
    # Theme persistence (client-local)
    # ------------------------------------------------------------------

    def watch_theme(self, theme: str) -> None:
        self._local_storage.save_theme(theme)

    def _restore_panel_widths(self) -> None:
        widths = self._local_storage.load_panel_widths()
        for panel_id, width in widths.items():
            try:
                self.query_one(f"#{panel_id}").styles.width = width
            except Exception:
                pass

    def _save_panel_width(self, panel_id: str, width: int) -> None:
        self._local_storage.save_panel_width(panel_id, width)

    # ------------------------------------------------------------------
    # Event handlers
    # ------------------------------------------------------------------

    def on_workspace_new_requested(self, event: WorkspaceNewRequested) -> None:
        self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)

    def on_workspace_selected(self, event: WorkspaceSelected) -> None:
        # Unsubscribe old room
        if self._current_workspace and self._current_room:
            self._unsubscribe_room(self._current_workspace.name, self._current_room.name)

        async def _do() -> None:
            await self._select_workspace(event.name)
            last_room_name = self._local_storage.load_last_room(event.name)
            if last_room_name:
                await self._select_room(last_room_name)
            else:
                self._current_room = None
                self._refresh_agents_sync()
                chat = self.query_one(ChatPanel)
                chat.set_workspace_path(self._current_workspace.root_path if self._current_workspace else None)
                chat.set_room(None)
                chat.clear_messages()

        self.run_worker(_do, thread=False)

    def on_room_new_requested(self, event: RoomNewRequested) -> None:
        if not self._current_workspace:
            self.notify("Select a workspace first.", severity="warning")
            return
        self.push_screen(CreateRoomScreen(), self._on_room_created)

    def on_room_selected(self, event: RoomSelected) -> None:
        if not self._current_workspace:
            return
        # Unsubscribe old room
        if self._current_room:
            self._unsubscribe_room(self._current_workspace.name, self._current_room.name)

        async def _do() -> None:
            await self._select_room(event.name)
            if self._current_workspace and self._current_room:
                self._local_storage.save_last_session(
                    self._current_workspace.name, self._current_room.name
                )

        self.run_worker(_do, thread=False)

    async def on_message_submitted(self, event: MessageSubmitted) -> None:
        text = event.text

        if text.startswith("/"):
            cmd = text.strip().split()[0].lower()
            if cmd in {"/invite", "/kick", "/agents", "/rooms", "/status", "/help", "/skills", "/compact"}:
                await self._handle_command(text)
                return

        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return

        await self._conn.send({
            "type": "submit_message",
            "workspace": self._current_workspace.name,
            "room": self._current_room.name,
            "text": text,
        })

    # ------------------------------------------------------------------
    # Commands (client-side handling that delegates to server)
    # ------------------------------------------------------------------

    async def _handle_command(self, text: str) -> None:
        parts = text.strip().split(maxsplit=2)
        cmd = parts[0].lower()

        if cmd == "/invite":
            await self._cmd_invite(parts)
        elif cmd == "/kick":
            await self._cmd_kick(parts)
        elif cmd == "/agents":
            await self._cmd_list_agents()
        elif cmd == "/rooms":
            await self._cmd_list_rooms()
        elif cmd == "/status":
            self._cmd_status()
        elif cmd == "/help":
            self._cmd_help()
        elif cmd == "/compact":
            instructions = text.strip()[len("/compact"):].strip()
            await self._cmd_compact(instructions)
        else:
            self.notify(f"Unknown command: {cmd}", severity="error")

    async def _cmd_invite(self, parts: list[str]) -> None:
        if len(parts) < 2:
            self.notify("Usage: /invite <agent-name>", severity="error")
            return
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return
        reply = await self._conn.send({
            "type": "invite_agent",
            "workspace": self._current_workspace.name,
            "room": self._current_room.name,
            "agent_name": parts[1],
        })
        if reply and reply.get("type") == "ok" and reply.get("room"):
            self._current_room = Room.from_dict(reply["room"])
            self._refresh_agents_sync()
        elif reply and reply.get("type") == "error":
            self.notify(reply["message"], severity="error")

    async def _cmd_kick(self, parts: list[str]) -> None:
        if len(parts) < 2:
            self.notify("Usage: /kick <agent-name>", severity="error")
            return
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return
        reply = await self._conn.send({
            "type": "kick_agent",
            "workspace": self._current_workspace.name,
            "room": self._current_room.name,
            "agent_name": parts[1],
        })
        if reply and reply.get("type") == "ok" and reply.get("room"):
            self._current_room = Room.from_dict(reply["room"])
            self._refresh_agents_sync()
        elif reply and reply.get("type") == "error":
            self.notify(reply["message"], severity="error")

    async def _cmd_list_agents(self) -> None:
        reply = await self._conn.send({"type": "list_agents"})
        if reply and reply.get("type") == "agent_list":
            agents = reply["agents"]
            if agents:
                lines = [f"  {a['name']} - {a.get('description', '')}" for a in agents]
                self.notify("Agents:\n" + "\n".join(lines))
            else:
                self.notify("No agents defined.")

    async def _cmd_list_rooms(self) -> None:
        if not self._current_workspace:
            self.notify("Select a workspace first.", severity="warning")
            return
        reply = await self._conn.send({"type": "list_rooms", "workspace": self._current_workspace.name})
        if reply and reply.get("type") == "room_list":
            rooms = reply["rooms"]
            if rooms:
                lines = [f"  {r['name']} (leader: {r['leader']})" for r in rooms]
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
            "  /invite <agent>  -- Invite agent to room\n"
            "  /kick <agent>    -- Remove agent from room\n"
            "  /agents          -- List all agents\n"
            "  /rooms           -- List rooms in workspace\n"
            "  /status          -- Show current state\n"
            "  /compact [hint]  -- Summarize & clear history\n"
            "  /help            -- Show this help\n"
            "\n"
            "Keys:\n"
            "  Ctrl+N -- Create new item\n"
            "  Ctrl+D -- Delete selected item"
        )

    async def _cmd_compact(self, instructions: str = "") -> None:
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return
        reply = await self._conn.send({
            "type": "compact",
            "workspace": self._current_workspace.name,
            "room": self._current_room.name,
            "instructions": instructions,
        })
        if reply and reply.get("type") == "error":
            self.notify(reply["message"], severity="error")

    # ------------------------------------------------------------------
    # Actions
    # ------------------------------------------------------------------

    def action_create_new(self) -> None:
        focused = self.focused
        if focused is None:
            self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)
            return

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

        self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)

    def _on_workspace_created(self, result: Workspace | None) -> None:
        if result:
            async def _do() -> None:
                await self._conn.send({
                    "type": "create_workspace",
                    "name": result.name,
                    "root_path": result.root_path,
                })
                self._current_workspace = result
                self._current_room = None
                await self._refresh_workspaces()
                await self._refresh_rooms()
                await self._refresh_agents()
                await self._refresh_chat()
                self.notify(f"Workspace '{result.name}' created.")

            self.run_worker(_do, thread=False)

    def _on_room_created(self, result: RoomCreateResult | None) -> None:
        if result and self._current_workspace:
            ws = self._current_workspace

            async def _do() -> None:
                reply = await self._conn.send({
                    "type": "create_room",
                    "workspace": ws.name,
                    "room_name": result.name,
                })
                if reply and reply.get("type") == "ok" and reply.get("room"):
                    self._current_room = Room.from_dict(reply["room"])
                    await self._refresh_rooms()
                    self._refresh_agents_sync()
                    await self._refresh_chat()
                    self._subscribe_current_room()
                    self._local_storage.save_last_session(ws.name, self._current_room.name)
                    self.notify(f"Room '{result.name}' created.")

            self.run_worker(_do, thread=False)

    def on_agent_edit_requested(self, event: AgentEditRequested) -> None:
        async def _do() -> None:
            reply = await self._conn.send({"type": "get_agent", "name": event.name})
            if not reply or reply.get("type") != "agent_info" or not reply.get("agent"):
                self.notify(f"Agent '{event.name}' not found.", severity="error")
                return
            agent = AgentConfig.from_dict(reply["agent"])

            def _on_saved(result: object) -> None:
                if isinstance(result, AgentConfig):
                    async def _save() -> None:
                        await self._conn.send({"type": "create_agent", "agent": result.to_dict()})
                        self._refresh_agents_sync()
                        self.notify(f"Agent '{result.name}' saved.")
                    self.run_worker(_save, thread=False)

            self.push_screen(CreateAgentScreen(existing=agent, total_tokens=0), _on_saved)

        self.run_worker(_do, thread=False)

    def _on_agent_created(self, result: object) -> None:
        from humu.models.agent import AgentConfig
        if isinstance(result, AgentConfig):
            async def _do() -> None:
                await self._conn.send({"type": "create_agent", "agent": result.to_dict()})
                self._refresh_agents_sync()
                self.notify(f"Agent '{result.name}' created.")
            self.run_worker(_do, thread=False)

    def action_delete_selected(self) -> None:
        from humu.tui.screens.confirm import ConfirmScreen

        focused = self.focused
        if focused is None:
            return

        node = focused
        while node is not None:
            if isinstance(node, WorkspacePanel):
                if self._current_workspace:
                    name = self._current_workspace.name
                    self.push_screen(
                        ConfirmScreen(f"Delete workspace '{name}'?\nThis cannot be undone."),
                        lambda confirmed, _name=name: self._delete_workspace(_name) if confirmed else None,
                    )
                return
            if isinstance(node, RoomPanel):
                if self._current_workspace and self._current_room:
                    name = self._current_room.name
                    self.push_screen(
                        ConfirmScreen(f"Delete room '{name}'?\nThis cannot be undone."),
                        lambda confirmed, _name=name: self._delete_room(_name) if confirmed else None,
                    )
                return
            node = node.parent

    def _delete_workspace(self, name: str) -> None:
        async def _do() -> None:
            await self._conn.send({"type": "delete_workspace", "name": name})
            self._current_workspace = None
            self._current_room = None
            await self._refresh_workspaces()
            await self._refresh_rooms()
            await self._refresh_agents()
            await self._refresh_chat()
            self.notify(f"Workspace '{name}' deleted.")
        self.run_worker(_do, thread=False)

    def _delete_room(self, name: str) -> None:
        if not self._current_workspace:
            return
        ws = self._current_workspace

        async def _do() -> None:
            await self._conn.send({"type": "delete_room", "workspace": ws.name, "room_name": name})
            self._current_room = None
            await self._refresh_rooms()
            await self._refresh_agents()
            await self._refresh_chat()
            self.notify(f"Room '{name}' deleted.")
        self.run_worker(_do, thread=False)

    def action_quit_or_warn(self) -> None:
        if self._quit_pending:
            self.exit()
            return

        from humu.tui.widgets.chat_panel import ChatInput
        chat_input = self.query_one("#chat-input", ChatInput)
        if chat_input.text:
            chat_input.load_text("")
        else:
            self._quit_pending = True
            self.notify("Press Ctrl+C again to quit.", timeout=2)
            self._quit_timer = self.set_timer(2, self._reset_quit)

    def _reset_quit(self) -> None:
        self._quit_pending = False

    def action_plugin_manager(self) -> None:
        from humu.tui.screens.plugin_manager import PluginManagerScreen
        self.push_screen(PluginManagerScreen(self._local_storage))

    def action_cancel_processing(self) -> None:
        if not self._current_workspace or not self._current_room:
            return

        async def _do() -> None:
            await self._conn.send({
                "type": "cancel_processing",
                "workspace": self._current_workspace.name,
                "room": self._current_room.name,
            })
        self.run_worker(_do, thread=False)

    def action_restart(self) -> None:
        self.exit(result=RELOAD_EXIT_CODE)
