"""WebSocket client — connects TUI to the backend server."""

from __future__ import annotations

import asyncio
import json
import logging
from collections.abc import Callable
from typing import Any

import websockets
import websockets.exceptions
from websockets.asyncio.client import ClientConnection, unix_connect

from humu.config import HUMU_HOME

logger = logging.getLogger(__name__)

SOCKET_PATH = HUMU_HOME / "humu.sock"


class Connection:
    """Async WebSocket client for the Humu server.

    Usage::

        conn = Connection()
        await conn.connect()
        conn.on_event = my_callback   # called with event dict
        reply = await conn.send({"type": "list_workspaces"})
    """

    def __init__(self) -> None:
        self._ws: ClientConnection | None = None
        self._reader_task: asyncio.Task | None = None
        # Pending request-reply pairs keyed by command type
        self._pending: dict[str, asyncio.Future[dict]] = {}
        # Callback for broadcast events (not replies)
        self.on_event: Callable[[dict], Any] | None = None

    @property
    def connected(self) -> bool:
        return self._ws is not None

    async def connect(self) -> None:
        """Connect to the Humu server via Unix domain socket."""
        self._ws = await unix_connect(
            uri="ws://localhost/",
            path=str(SOCKET_PATH),
            max_size=64 * 1024 * 1024,
        )
        self._reader_task = asyncio.create_task(self._read_loop())
        logger.info("Connected to server at %s", SOCKET_PATH)

    async def disconnect(self) -> None:
        if self._reader_task:
            self._reader_task.cancel()
            try:
                await self._reader_task
            except (asyncio.CancelledError, Exception):
                pass
            self._reader_task = None
        if self._ws:
            await self._ws.close()
            self._ws = None

    async def send(self, msg: dict) -> dict | None:
        """Send a command and optionally wait for a reply.

        Returns the server's reply dict, or None if no reply is expected.
        """
        if not self._ws:
            raise RuntimeError("Not connected")

        msg_type = msg.get("type", "")
        await self._ws.send(json.dumps(msg))

        # Subscribe/unsubscribe don't get replies
        if msg_type in {"subscribe_room", "unsubscribe_room"}:
            return None

        # Wait for a reply (matched by expected response type)
        future: asyncio.Future[dict] = asyncio.get_running_loop().create_future()
        self._pending[msg_type] = future

        try:
            return await asyncio.wait_for(future, timeout=300)
        except asyncio.TimeoutError:
            self._pending.pop(msg_type, None)
            return {"type": "error", "message": "Request timed out"}

    async def send_nowait(self, msg: dict) -> None:
        """Send a command without waiting for a reply."""
        if not self._ws:
            raise RuntimeError("Not connected")
        await self._ws.send(json.dumps(msg))

    async def _read_loop(self) -> None:
        """Read messages from server and dispatch them."""
        assert self._ws is not None
        try:
            async for raw in self._ws:
                try:
                    event = json.loads(raw)
                except (json.JSONDecodeError, TypeError):
                    continue

                event_type = event.get("type", "")

                # Check if this is a reply to a pending request
                resolved = False
                for cmd_type, future in list(self._pending.items()):
                    if self._is_reply_for(cmd_type, event_type):
                        future.set_result(event)
                        self._pending.pop(cmd_type, None)
                        resolved = True
                        break

                if not resolved and self.on_event:
                    self.on_event(event)

        except websockets.exceptions.ConnectionClosed:
            logger.warning("Connection to server lost")
        except asyncio.CancelledError:
            pass
        except Exception:
            logger.exception("Read loop error")

    @staticmethod
    def _is_reply_for(cmd_type: str, event_type: str) -> bool:
        """Determine if an event is a direct reply to a command."""
        reply_map = {
            "list_workspaces": "workspace_list",
            "list_rooms": "room_list",
            "list_agents": "agent_list",
            "get_agent": "agent_info",
            "get_chat_history": "chat_history",
            "get_skills": "skills_list",
            "get_processing_state": "processing_state",
            "create_workspace": "ok",
            "delete_workspace": "ok",
            "create_room": "ok",
            "delete_room": "ok",
            "create_agent": "ok",
            "invite_agent": "ok",
            "kick_agent": "ok",
            "submit_message": "ok",
            "cancel_processing": "ok",
        }
        expected = reply_map.get(cmd_type)
        return event_type == expected or event_type == "error"
