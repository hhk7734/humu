"""Command handler — bridges WebSocket commands to backend services."""

from __future__ import annotations

import asyncio
import logging
from collections.abc import Callable
from typing import Any

from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.services.agent_runner import AgentRunner
from humu.services.router import Router
from humu.services.storage import Storage

logger = logging.getLogger(__name__)


class Handler:
    """Processes client commands, yields server events.

    ``broadcast`` is called with ``(event_dict, workspace, room)`` so the
    server can route events to the correct subscribers.
    """

    def __init__(
        self,
        storage: Storage,
        runner: AgentRunner,
        router: Router,
        broadcast: Callable[[dict, str | None, str | None], Any],
    ) -> None:
        self._storage = storage
        self._runner = runner
        self._router = router
        self._broadcast = broadcast

        # Track processing state across all rooms
        self._processing: set[tuple[str, str]] = set()
        self._pending_messages: dict[tuple[str, str], list[str]] = {}
        self._active_tasks: dict[tuple[str, str], asyncio.Task[None]] = {}

        # Wire up system event callback
        self._router.on_system_event = self._on_system_event

    # ------------------------------------------------------------------
    # Public dispatch
    # ------------------------------------------------------------------

    async def handle(self, msg: dict) -> dict | None:
        """Handle a single client command.  Returns a direct reply or None."""
        msg_type = msg.get("type", "")
        handler = getattr(self, f"_cmd_{msg_type}", None)
        if handler is None:
            return {"type": "error", "message": f"Unknown command: {msg_type}"}
        try:
            return await handler(msg)
        except Exception as e:
            logger.exception("Handler error for %s", msg_type)
            return {"type": "error", "message": str(e), "request_type": msg_type}

    # ------------------------------------------------------------------
    # Query commands (return data directly to the requesting client)
    # ------------------------------------------------------------------

    async def _cmd_list_workspaces(self, msg: dict) -> dict:
        workspaces = self._storage.list_workspaces()
        return {
            "type": "workspace_list",
            "workspaces": [w.to_dict() for w in workspaces],
        }

    async def _cmd_list_rooms(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        rooms = self._storage.list_rooms(ws)
        return {
            "type": "room_list",
            "workspace": ws.name,
            "rooms": [r.to_dict() for r in rooms],
        }

    async def _cmd_list_agents(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        agents = self._storage.list_agents(ws)
        return {
            "type": "agent_list",
            "agents": [a.to_dict() for a in agents],
        }

    async def _cmd_get_agent(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        agent = self._storage.get_agent(ws, msg["name"])
        return {
            "type": "agent_info",
            "agent": agent.to_dict() if agent else None,
        }

    async def _cmd_get_chat_history(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        history = self._storage.load_chat_history(ws, msg["room"])
        return {
            "type": "chat_history",
            "workspace": ws.name,
            "room": msg["room"],
            "messages": history,
        }

    async def _cmd_get_skills(self, msg: dict) -> dict:
        skills = self._storage.list_skills()
        return {"type": "skills_list", "skills": skills}

    async def _cmd_get_processing_state(self, msg: dict) -> dict:
        """Return the set of currently processing rooms."""
        return {
            "type": "processing_state",
            "rooms": [{"workspace": ws, "room": rm} for ws, rm in self._processing],
        }

    # ------------------------------------------------------------------
    # Mutation commands
    # ------------------------------------------------------------------

    async def _cmd_create_workspace(self, msg: dict) -> dict:
        ws = Workspace(name=msg["name"], root_path=msg["root_path"])
        self._storage.save_workspace(ws)
        self._broadcast(
            {
                "type": "workspace_list",
                "workspaces": [w.to_dict() for w in self._storage.list_workspaces()],
            },
            None,
            None,
        )
        return {"type": "ok"}

    async def _cmd_delete_workspace(self, msg: dict) -> dict:
        self._storage.delete_workspace(msg["name"])
        self._broadcast(
            {
                "type": "workspace_list",
                "workspaces": [w.to_dict() for w in self._storage.list_workspaces()],
            },
            None,
            None,
        )
        return {"type": "ok"}

    async def _cmd_create_room(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        room_name = msg["room_name"]
        leader_name = f"{room_name}-leader"
        if not self._storage.get_agent(ws, leader_name):
            leader = AgentConfig(
                name=leader_name,
                description=f"Leader agent for room '{room_name}'. Routes messages to the right agents.",
                prompt=(
                    "You are a helpful leader agent. "
                    "Read user messages and decide whether to answer directly "
                    "or forward to a specialist agent in the room."
                ),
            )
            self._storage.save_agent(ws, leader)
        room = Room(name=room_name, leader=leader_name)
        self._storage.save_room(ws, room)
        self._broadcast(
            {
                "type": "room_list",
                "workspace": ws.name,
                "rooms": [r.to_dict() for r in self._storage.list_rooms(ws)],
            },
            ws.name,
            None,
        )
        return {"type": "ok", "room": room.to_dict()}

    async def _cmd_delete_room(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        self._storage.delete_room(ws, msg["room_name"])
        self._broadcast(
            {
                "type": "room_list",
                "workspace": ws.name,
                "rooms": [r.to_dict() for r in self._storage.list_rooms(ws)],
            },
            ws.name,
            None,
        )
        return {"type": "ok"}

    async def _cmd_create_agent(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        agent = AgentConfig.from_dict(msg["agent"])
        self._storage.save_agent(ws, agent)
        self._broadcast(
            {
                "type": "agent_list",
                "agents": [a.to_dict() for a in self._storage.list_agents(ws)],
            },
            ws.name,
            None,
        )
        return {"type": "ok"}

    async def _cmd_invite_agent(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        room = self._storage.get_room(ws, msg["room"])
        if not room:
            return {"type": "error", "message": f"Room '{msg['room']}' not found"}
        agent_name = msg["agent_name"]
        if not self._storage.get_agent(ws, agent_name):
            return {"type": "error", "message": f"Agent '{agent_name}' not found"}
        if agent_name in room.agents or agent_name == room.leader:
            return {"type": "error", "message": f"Agent '{agent_name}' already in room"}
        room.agents.append(agent_name)
        self._storage.save_room(ws, room)
        self._broadcast(
            {
                "type": "message_added",
                "workspace": ws.name,
                "room": room.name,
                "sender": "system",
                "text": f"Invited {agent_name} to the room.",
                "is_system": True,
                "raw": None,
                "steps": [],
            },
            ws.name,
            room.name,
        )
        return {"type": "ok", "room": room.to_dict()}

    async def _cmd_kick_agent(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        room = self._storage.get_room(ws, msg["room"])
        if not room:
            return {"type": "error", "message": f"Room '{msg['room']}' not found"}
        agent_name = msg["agent_name"]
        if agent_name == room.leader:
            return {"type": "error", "message": "Cannot kick the leader agent"}
        if agent_name not in room.agents:
            return {"type": "error", "message": f"Agent '{agent_name}' not in room"}
        room.agents.remove(agent_name)
        self._storage.save_room(ws, room)
        self._broadcast(
            {
                "type": "message_added",
                "workspace": ws.name,
                "room": room.name,
                "sender": "system",
                "text": f"Kicked {agent_name} from the room.",
                "is_system": True,
                "raw": None,
                "steps": [],
            },
            ws.name,
            room.name,
        )
        return {"type": "ok", "room": room.to_dict()}

    # ------------------------------------------------------------------
    # Message submission
    # ------------------------------------------------------------------

    async def _cmd_submit_message(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        room = self._storage.get_room(ws, msg["room"])
        if not room:
            return {"type": "error", "message": f"Room '{msg['room']}' not found"}

        text = msg["text"]
        room_key = (ws.name, room.name)

        if room_key in self._processing:
            self._pending_messages.setdefault(room_key, []).append(text)
            self._broadcast(
                {
                    "type": "queue_updated",
                    "workspace": ws.name,
                    "room": room.name,
                    "pending_count": len(self._pending_messages[room_key]),
                    "pending_messages": list(self._pending_messages[room_key]),
                },
                ws.name,
                room.name,
            )
            return {"type": "ok", "queued": True}

        asyncio.create_task(self._process_message(ws, room, text))
        return {"type": "ok", "queued": False}

    async def _cmd_cancel_processing(self, msg: dict) -> dict:
        room_key = (msg["workspace"], msg["room"])
        task = self._active_tasks.get(room_key)
        if task:
            task.cancel()
        return {"type": "ok"}

    async def _cmd_compact(self, msg: dict) -> dict:
        ws = self._storage.get_workspace(msg["workspace"])
        if not ws:
            return {
                "type": "error",
                "message": f"Workspace '{msg['workspace']}' not found",
            }
        room = self._storage.get_room(ws, msg["room"])
        if not room:
            return {"type": "error", "message": f"Room '{msg['room']}' not found"}
        room_key = (ws.name, room.name)
        if room_key in self._processing:
            return {"type": "error", "message": "Cannot compact while processing"}

        instructions = msg.get("instructions", "")
        asyncio.create_task(self._do_compact(ws, room, instructions))
        return {"type": "ok"}

    # ------------------------------------------------------------------
    # These are no-ops on server — clients track subscriptions in server.py
    # ------------------------------------------------------------------

    async def _cmd_subscribe_room(self, msg: dict) -> dict | None:
        return None  # Handled by server.py

    async def _cmd_unsubscribe_room(self, msg: dict) -> dict | None:
        return None  # Handled by server.py

    # ------------------------------------------------------------------
    # Internal processing
    # ------------------------------------------------------------------

    async def _process_message(
        self,
        workspace: Workspace,
        room: Room,
        text: str,
    ) -> None:
        room_key = (workspace.name, room.name)
        self._processing.add(room_key)
        self._active_tasks[room_key] = asyncio.current_task()  # type: ignore[arg-type]

        # Broadcast "you" message
        self._storage.append_chat_message(
            workspace, room.name, {"sender": "you", "text": text}
        )
        self._broadcast(
            {
                "type": "message_added",
                "workspace": workspace.name,
                "room": room.name,
                "sender": "you",
                "text": text,
                "is_system": False,
                "raw": None,
                "steps": [],
            },
            workspace.name,
            room.name,
        )

        cancelled = False
        try:
            async for msg in self._router.handle_message(workspace, room, text):
                if msg.is_loading:
                    self._broadcast(
                        {
                            "type": "processing_started",
                            "workspace": workspace.name,
                            "room": room.name,
                            "sender": msg.sender,
                        },
                        workspace.name,
                        room.name,
                    )
                    continue

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
                self._broadcast(
                    {
                        "type": "message_added",
                        "workspace": workspace.name,
                        "room": room.name,
                        "sender": msg.sender,
                        "text": msg.text,
                        "is_system": msg.is_system,
                        "raw": msg.raw,
                        "steps": msg.steps,
                    },
                    workspace.name,
                    room.name,
                )
        except asyncio.CancelledError:
            cancel_msg = {
                "sender": room.leader,
                "text": "Cancelled",
                "is_system": False,
                "raw": None,
                "steps": [],
            }
            self._storage.append_chat_message(workspace, room.name, cancel_msg)
            self._broadcast(
                {
                    "type": "processing_cancelled",
                    "workspace": workspace.name,
                    "room": room.name,
                },
                workspace.name,
                room.name,
            )
            self._broadcast(
                {
                    "type": "message_added",
                    "workspace": workspace.name,
                    "room": room.name,
                    "sender": room.leader,
                    "text": "Cancelled",
                    "is_system": False,
                    "raw": None,
                    "steps": [],
                },
                workspace.name,
                room.name,
            )
        except Exception as e:
            import traceback

            err_detail = f"{e}\n{traceback.format_exc()}"
            self._storage.append_chat_message(
                workspace,
                room.name,
                {
                    "sender": "error",
                    "text": err_detail,
                    "is_system": True,
                    "raw": None,
                    "steps": [],
                },
            )
            self._broadcast(
                {
                    "type": "message_added",
                    "workspace": workspace.name,
                    "room": room.name,
                    "sender": "error",
                    "text": err_detail,
                    "is_system": True,
                    "raw": None,
                    "steps": [],
                },
                workspace.name,
                room.name,
            )
        finally:
            self._processing.discard(room_key)
            self._active_tasks.pop(room_key, None)
            self._broadcast(
                {
                    "type": "processing_done",
                    "workspace": workspace.name,
                    "room": room.name,
                },
                workspace.name,
                room.name,
            )
            self._process_next_queued(workspace, room)

    def _process_next_queued(self, workspace: Workspace, room: Room) -> None:
        room_key = (workspace.name, room.name)
        queue = self._pending_messages.get(room_key, [])
        if not queue:
            return
        next_text = queue.pop(0)
        if not queue:
            self._pending_messages.pop(room_key, None)
        self._broadcast(
            {
                "type": "queue_updated",
                "workspace": workspace.name,
                "room": room.name,
                "pending_count": len(self._pending_messages.get(room_key, [])),
                "pending_messages": list(self._pending_messages.get(room_key, [])),
            },
            workspace.name,
            room.name,
        )
        asyncio.create_task(self._process_message(workspace, room, next_text))

    async def _do_compact(
        self, workspace: Workspace, room: Room, instructions: str
    ) -> None:
        room_key = (workspace.name, room.name)
        self._processing.add(room_key)
        self._active_tasks[room_key] = asyncio.current_task()  # type: ignore[arg-type]

        self._broadcast(
            {
                "type": "message_added",
                "workspace": workspace.name,
                "room": room.name,
                "sender": "system",
                "text": "Compacting conversation history...",
                "is_system": True,
                "raw": None,
                "steps": [],
            },
            workspace.name,
            room.name,
        )

        try:
            history = self._storage.load_chat_history(workspace, room.name)
            lines: list[str] = []
            for m in history:
                sender = m.get("sender", "unknown")
                text = m.get("text", "")
                if m.get("is_system"):
                    lines.append(f"[system] {text}")
                else:
                    lines.append(f"[{sender}] {text}")
            conversation_text = "\n".join(lines)

            prompt = (
                "Summarize the following conversation concisely. "
                "Capture the key topics discussed, decisions made, important context, "
                "and any pending tasks or action items. "
                "Write the summary in the same language as the conversation.\n"
            )
            if instructions:
                prompt += f"\nAdditional instructions: {instructions}\n"
            prompt += f"\n---\n{conversation_text}\n---"

            leader_cfg = self._storage.get_agent(workspace, room.leader)
            if not leader_cfg:
                self._broadcast(
                    {"type": "error", "message": "Leader agent not found"},
                    workspace.name,
                    room.name,
                )
                return

            response = await self._runner.query(
                leader_cfg, workspace, room.name, prompt
            )
            summary_text = response.text

            # Clear sessions
            all_agents = [room.leader] + list(room.agents)
            for agent_name in all_agents:
                self._storage.delete_session_id(workspace, room.name, agent_name)

            summary_msg = {
                "sender": "system",
                "text": f"--- Conversation Summary ---\n{summary_text}",
                "is_system": True,
                "raw": None,
                "steps": [],
            }
            self._storage.replace_chat_history(workspace, room.name, [summary_msg])

            # Tell clients to reload history
            self._broadcast(
                {
                    "type": "chat_history",
                    "workspace": workspace.name,
                    "room": room.name,
                    "messages": [summary_msg],
                },
                workspace.name,
                room.name,
            )
        except Exception as e:
            logger.exception("Compact failed")
            self._broadcast(
                {"type": "error", "message": f"Compact failed: {e}"},
                workspace.name,
                room.name,
            )
        finally:
            self._processing.discard(room_key)
            self._active_tasks.pop(room_key, None)
            self._broadcast(
                {
                    "type": "processing_done",
                    "workspace": workspace.name,
                    "room": room.name,
                },
                workspace.name,
                room.name,
            )

    # ------------------------------------------------------------------
    # System event callback (from Router)
    # ------------------------------------------------------------------

    def _on_system_event(
        self, room_key: tuple[str, str], agent_name: str, step: dict
    ) -> None:
        subtype = step.get("subtype", "")
        if subtype in {"init", "task_started", "task_progress", "task_notification"}:
            return
        data = step.get("data", {})
        summary = (
            data.get("summary") or data.get("description") or data.get("message") or ""
        )
        text = (
            f"System: {summary}"
            if summary
            else f"System event: {subtype}"
            if subtype
            else "System event"
        )
        sender = agent_name or "system"
        ws_name, room_name = room_key

        self._broadcast(
            {
                "type": "system_event",
                "workspace": ws_name,
                "room": room_name,
                "agent": sender,
                "text": text,
            },
            ws_name,
            room_name,
        )
