from __future__ import annotations

import asyncio

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.widgets import Footer, Header

from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.services.agent_runner import AgentRunner
from humu.services.router import Router
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
        self._storage = Storage()
        self._runner = AgentRunner(self._storage)
        self._router = Router(self._runner, self._storage)
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
        # Per-room message queue (messages waiting while room is processing)
        self._pending_messages: dict[tuple[str, str], list[str]] = {}
        # Per-room (event_loop, asyncio.Task) for cancellation support
        self._active_tasks: dict[
            tuple[str, str],
            tuple[asyncio.AbstractEventLoop, "asyncio.Task[None]"],
        ] = {}

    def compose(self) -> ComposeResult:
        yield Header()
        with Horizontal(id="main-layout"):
            yield WorkspacePanel(id="workspace-panel")
            yield ResizeHandle("workspace-panel", min_width=10, save_callback=self._save_panel_width)
            yield RoomPanel(id="room-panel")
            yield ResizeHandle("room-panel", min_width=8, save_callback=self._save_panel_width)
            yield ChatPanel(get_skills=self._storage.list_skills)
            yield ResizeHandle("agent-panel", min_width=10, invert=True, save_callback=self._save_panel_width)
            yield AgentPanel(id="agent-panel")
        yield Footer()

    def on_mount(self) -> None:
        saved_theme = self._storage.load_theme()
        if saved_theme:
            self.theme = saved_theme
        self.query_one(Header).icon = "Menu"
        self._router.on_system_event = self._on_system_event
        self._refresh_workspaces()
        self._restore_last_session()
        self._restore_panel_widths()

    # System event subtypes to ignore (session lifecycle, not user-relevant)
    _IGNORED_SYSTEM_SUBTYPES = {"init", "task_started", "task_progress", "task_notification"}

    def _on_system_event(self, room_key: tuple[str, str], agent_name: str, step: dict) -> None:
        """Called from worker thread when a system event (e.g. compaction) is detected."""
        subtype = step.get("subtype", "")
        data = step.get("data", {})

        # Skip routine lifecycle events
        if subtype in self._IGNORED_SYSTEM_SUBTYPES:
            return

        # Build a descriptive message from the event data
        summary = data.get("summary") or data.get("description") or data.get("message") or ""
        if summary:
            text = f"🔄 {summary}"
        else:
            text = f"🔄 System event: {subtype}" if subtype else "🔄 System event"

        sender = agent_name or "system"

        def _show() -> None:
            self._storage.append_chat_message(
                self._current_workspace,
                room_key[1],
                {"sender": sender, "text": text, "is_system": False, "raw": None, "steps": []},
            )
            if (
                self._current_workspace
                and self._current_room
                and room_key == (self._current_workspace.name, self._current_room.name)
            ):
                self.query_one(ChatPanel).add_message(sender, text)

        self.call_from_thread(_show)

    def watch_theme(self, theme: str) -> None:
        """Persist theme selection across restarts."""
        self._storage.save_theme(theme)

    def _restore_panel_widths(self) -> None:
        widths = self._storage.load_panel_widths()
        for panel_id, width in widths.items():
            try:
                self.query_one(f"#{panel_id}").styles.width = width
            except Exception:
                pass

    def _save_panel_width(self, panel_id: str, width: int) -> None:
        self._storage.save_panel_width(panel_id, width)

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
        processing_ws = {ws for ws, _ in self._processing}
        spinner = self._spinner_frames[self._spinner_frame]
        self.query_one(WorkspacePanel).set_workspaces(names, selected, processing_ws, spinner)

    def _refresh_rooms(self) -> None:
        room_panel = self.query_one(RoomPanel)
        if not self._current_workspace:
            room_panel.set_rooms([])
            return
        rooms = self._storage.list_rooms(self._current_workspace)
        names = [r.name for r in rooms]
        selected = self._current_room.name if self._current_room else None
        ws_name = self._current_workspace.name
        processing_rooms = {room for ws, room in self._processing if ws == ws_name}
        spinner = self._spinner_frames[self._spinner_frame]
        room_panel.set_rooms(names, selected, processing_rooms, spinner)

    def _start_spinner(self) -> None:
        if self._spinner_timer is None:
            self._spinner_timer = self.set_interval(0.1, self._tick_spinner)
        self._refresh_workspaces()
        self._refresh_rooms()

    def _stop_spinner(self) -> None:
        if self._spinner_timer is not None:
            self._spinner_timer.stop()
            self._spinner_timer = None
        self._refresh_workspaces()
        self._refresh_rooms()

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

    def _refresh_agents(self) -> None:
        agent_panel = self.query_one(AgentPanel)
        if not self._current_room:
            agent_panel.set_agents(None)
            return
        agent_panel.set_agents(
            self._current_room.leader, self._current_room.agents
        )

    def _refresh_queue_display(self) -> None:
        """Update the pending queue display for the currently viewed room."""
        if not self._current_workspace or not self._current_room:
            self.query_one(ChatPanel).set_pending_queue([])
            return
        room_key = (self._current_workspace.name, self._current_room.name)
        queue = self._pending_messages.get(room_key, [])
        self.query_one(ChatPanel).set_pending_queue(queue)

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
        # Re-show loading indicator if this room is still processing
        room_key = (self._current_workspace.name, self._current_room.name)
        sender = self._active_loading.get(room_key)
        if sender:
            chat_panel.show_loading(sender, lambda: self._router.get_live_steps(room_key))
        self._refresh_queue_display()

    # --- Event handlers ---

    def on_workspace_new_requested(self, event: WorkspaceNewRequested) -> None:
        self.push_screen(CreateWorkspaceScreen(), self._on_workspace_created)

    def on_workspace_selected(self, event: WorkspaceSelected) -> None:
        ws = self._storage.get_workspace(event.name)
        if ws:
            self._current_workspace = ws
            last_room_name = self._storage.load_last_room(ws.name)
            self._current_room = (
                self._storage.get_room(ws, last_room_name) if last_room_name else None
            )
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
            cmd = text.strip().split()[0].lower()
            if cmd in {"/invite", "/kick", "/agents", "/rooms", "/status", "/help", "/skills", "/compact"}:
                await self._handle_command(text)
                return
            # Unrecognized /cmd — fall through to router as skill invocation

        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return

        workspace = self._current_workspace
        room = self._current_room
        room_key = (workspace.name, room.name)

        if room_key in self._processing:
            self._pending_messages.setdefault(room_key, []).append(text)
            self._refresh_queue_display()
            return

        self._start_room_processing(workspace, room, text)

    def _start_room_processing(self, workspace: Workspace, room: Room, text: str) -> None:
        room_key = (workspace.name, room.name)
        self._processing.add(room_key)
        self._start_spinner()
        if self._is_viewing(workspace, room):
            self.query_one(ChatPanel).add_message("you", text)
        self._storage.append_chat_message(
            workspace, room.name, {"sender": "you", "text": text},
        )

        def _process_sync() -> None:
            loop = asyncio.new_event_loop()
            try:
                task = loop.create_task(self._process_message(workspace, room, text))
                self._active_tasks[room_key] = (loop, task)
                loop.run_until_complete(task)
            except asyncio.CancelledError:
                pass
            finally:
                loop.close()
                self._active_tasks.pop(room_key, None)

        self.run_worker(_process_sync, thread=True)

    def _process_next_queued(self, workspace: Workspace, room: Room) -> None:
        room_key = (workspace.name, room.name)
        queue = self._pending_messages.get(room_key, [])
        if not queue:
            return
        next_text = queue.pop(0)
        if not queue:
            self._pending_messages.pop(room_key, None)
        self._refresh_queue_display()
        self._start_room_processing(workspace, room, next_text)

    def _is_viewing(self, workspace: Workspace, room: Room) -> bool:
        """Return True if the user is currently viewing this workspace+room."""
        return (
            self._current_workspace is not None
            and self._current_room is not None
            and self._current_workspace.name == workspace.name
            and self._current_room.name == room.name
        )

    async def _process_message(
        self,
        workspace: Workspace,
        room: Room,
        text: str,
    ) -> None:
        room_key = (workspace.name, room.name)

        def _show_loading(sender: str) -> None:
            self._active_loading[room_key] = sender
            if self._is_viewing(workspace, room):
                chat = self.query_one(ChatPanel)
                chat.show_loading(sender, lambda: self._router.get_live_steps(room_key))

        def _hide_loading() -> None:
            self._active_loading.pop(room_key, None)
            if self._is_viewing(workspace, room):
                chat = self.query_one(ChatPanel)
                chat.hide_loading()

        def _get_context_pct(agent_name: str) -> float | None:
            from humu.config import MODEL_CONTEXT_WINDOWS, DEFAULT_CONTEXT_WINDOW
            tokens = self._router.get_agent_tokens(workspace.name, room.name, agent_name)
            if tokens <= 0:
                return None
            agent_cfg = self._storage.get_agent(agent_name)
            model = agent_cfg.model if agent_cfg else ""
            ctx_size = MODEL_CONTEXT_WINDOWS.get(model, DEFAULT_CONTEXT_WINDOW)
            return min(tokens / ctx_size * 100, 100)

        def _add_message(sender: str, text: str, is_system: bool, raw: str | None, steps: list) -> None:
            self._storage.append_chat_message(
                workspace,
                room.name,
                {"sender": sender, "text": text, "is_system": is_system, "raw": raw, "steps": steps},
            )
            if self._is_viewing(workspace, room):
                chat = self.query_one(ChatPanel)
                pct = _get_context_pct(sender) if not is_system and sender != "you" else None
                chat.add_message(sender, text, is_system, raw, steps, context_pct=pct)

        cancelled = False
        try:
            async for msg in self._router.handle_message(workspace, room, text):
                if msg.is_loading:
                    self.call_from_thread(_show_loading, msg.sender)
                    continue
                self.call_from_thread(_hide_loading)
                self.call_from_thread(
                    _add_message, msg.sender, msg.text, msg.is_system, msg.raw, msg.steps,
                )
        except asyncio.CancelledError:
            cancelled = True
            # Drain the pending queue for this room on cancellation
            self._pending_messages.pop(room_key, None)
            self.call_from_thread(self._refresh_queue_display)
            self.call_from_thread(_add_message, room.leader, "⛔ Cancelled", False, None, [])
        except Exception as e:
            import traceback
            err_detail = f"{e}\n{traceback.format_exc()}"
            self.call_from_thread(
                _add_message, "error", err_detail, True, None, [],
            )
        finally:
            self.call_from_thread(_hide_loading)
            self._processing.discard(room_key)
            if not self._processing:
                self.call_from_thread(self._stop_spinner)
            else:
                self.call_from_thread(self._refresh_workspaces)
                self.call_from_thread(self._refresh_rooms)
            # Update agent panel with latest token usage
            if self._is_viewing(workspace, room):
                self.call_from_thread(self._refresh_agents)
            if not cancelled:
                self.call_from_thread(self._process_next_queued, workspace, room)

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
        elif cmd == "/compact":
            instructions = text.strip()[len("/compact"):].strip()
            self._cmd_compact(instructions)
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
            "  /compact [hint]  — Summarize & clear history\n"
            "  /help            — Show this help\n"
            "\n"
            "Keys:\n"
            "  Ctrl+N — Create new item\n"
            "  Ctrl+D — Delete selected item"
        )

    def _cmd_compact(self, instructions: str = "") -> None:
        """/compact — Summarize conversation history, clear it, and reset agent sessions."""
        if not self._current_workspace or not self._current_room:
            self.notify("Select a workspace and room first.", severity="warning")
            return

        workspace = self._current_workspace
        room = self._current_room
        room_key = (workspace.name, room.name)

        if room_key in self._processing:
            self.notify("Cannot compact while processing.", severity="warning")
            return

        history = self._storage.load_chat_history(workspace, room.name)
        if not history:
            self.notify("No conversation history to compact.", severity="warning")
            return

        chat = self.query_one(ChatPanel)
        chat.add_message("system", "Compacting conversation history...", is_system=True)

        def _do_compact() -> None:
            import asyncio as _asyncio

            # Build a text representation of the conversation
            lines: list[str] = []
            for msg in history:
                sender = msg.get("sender", "unknown")
                text = msg.get("text", "")
                if msg.get("is_system"):
                    lines.append(f"[system] {text}")
                else:
                    lines.append(f"[{sender}] {text}")
            conversation_text = "\n".join(lines)

            # Ask the leader agent to summarize
            prompt = (
                "Summarize the following conversation concisely. "
                "Capture the key topics discussed, decisions made, important context, "
                "and any pending tasks or action items. "
                "Write the summary in the same language as the conversation.\n"
            )
            if instructions:
                prompt += f"\nAdditional instructions: {instructions}\n"
            prompt += f"\n---\n{conversation_text}\n---"

            leader_cfg = self._storage.get_agent(room.leader)
            if not leader_cfg:
                self.call_from_thread(
                    lambda: self.notify("Leader agent not found.", severity="error")
                )
                return

            loop = _asyncio.new_event_loop()
            try:
                response = loop.run_until_complete(
                    self._runner.query(
                        leader_cfg, workspace, room.name, prompt,
                    )
                )
                summary_text = response.text
            except Exception as e:
                self.call_from_thread(
                    lambda: self.notify(f"Compact failed: {e}", severity="error")
                )
                return
            finally:
                loop.close()

            # Clear all agent session IDs for this room
            all_agents = [room.leader] + list(room.agents)
            for agent_name in all_agents:
                self._storage.delete_session_id(workspace, room.name, agent_name)

            # Clear token tracking
            self._router.clear_agent_tokens(workspace.name, room.name)

            # Replace chat history with just the summary
            summary_msg = {
                "sender": "system",
                "text": f"--- Conversation Summary ---\n{summary_text}",
                "is_system": True,
                "raw": None,
                "steps": [],
            }
            self._storage.replace_chat_history(workspace, room.name, [summary_msg])

            # Update UI
            def _update_ui() -> None:
                if self._is_viewing(workspace, room):
                    self._refresh_chat()
                    self._refresh_agents()
                self.notify("Conversation compacted.")

            self.call_from_thread(_update_ui)

        self.run_worker(_do_compact, thread=True)

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

    def on_agent_edit_requested(self, event: AgentEditRequested) -> None:
        agent = self._storage.get_agent(event.name)
        if not agent:
            self.notify(f"Agent '{event.name}' not found.", severity="error")
            return
        from humu.tui.screens.create_agent import CreateAgentScreen

        # Get token usage for this agent in current room
        total_tokens = 0
        if self._current_workspace and self._current_room:
            total_tokens = self._router.get_agent_tokens(
                self._current_workspace.name, self._current_room.name, event.name
            )

        def _on_saved(result: object) -> None:
            from humu.models.agent import AgentConfig

            if isinstance(result, AgentConfig):
                self._storage.save_agent(result)
                self._refresh_agents()
                self.notify(f"Agent '{result.name}' saved.")

        self.push_screen(CreateAgentScreen(existing=agent, total_tokens=total_tokens), _on_saved)

    def _on_agent_created(self, result: object) -> None:
        from humu.models.agent import AgentConfig

        if isinstance(result, AgentConfig):
            self._storage.save_agent(result)
            self._refresh_agents()
            self.notify(f"Agent '{result.name}' created.")

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
        self._storage.delete_workspace(name)
        self._current_workspace = None
        self._current_room = None
        self._refresh_workspaces()
        self._refresh_rooms()
        self._refresh_agents()
        self._refresh_chat()
        self.notify(f"Workspace '{name}' deleted.")

    def _delete_room(self, name: str) -> None:
        if not self._current_workspace:
            return
        self._storage.delete_room(self._current_workspace, name)
        self._current_room = None
        self._refresh_rooms()
        self._refresh_agents()
        self._refresh_chat()
        self.notify(f"Room '{name}' deleted.")

    def action_quit_or_warn(self) -> None:
        if self._quit_pending:
            self.exit()
            return

        # First Ctrl+C: clear chat input text
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

        self.push_screen(PluginManagerScreen(self._storage))

    def action_cancel_processing(self) -> None:
        """Cancel the active processing task for the current room (Escape)."""
        if not self._current_workspace or not self._current_room:
            return
        room_key = (self._current_workspace.name, self._current_room.name)
        entry = self._active_tasks.get(room_key)
        if entry:
            loop, task = entry
            loop.call_soon_threadsafe(task.cancel)

    def action_restart(self) -> None:
        self.exit(result=RELOAD_EXIT_CODE)
