from __future__ import annotations

import json
import logging

from fastapi import WebSocket

logger = logging.getLogger(__name__)


class WebSocketManager:
    def __init__(self) -> None:
        self._connections: set[WebSocket] = set()
        self._subscriptions: dict[WebSocket, set[tuple[str, str]]] = {}
        self._focused: dict[WebSocket, tuple[str, str] | None] = {}

    async def connect(self, ws: WebSocket) -> None:
        await ws.accept()
        self._connections.add(ws)
        self._subscriptions[ws] = set()
        self._focused[ws] = None

    def disconnect(self, ws: WebSocket) -> None:
        self._connections.discard(ws)
        self._subscriptions.pop(ws, None)
        self._focused.pop(ws, None)

    def subscribe(self, ws: WebSocket, workspace: str, room: str) -> None:
        self._subscriptions.setdefault(ws, set()).add((workspace, room))

    def unsubscribe(self, ws: WebSocket, workspace: str, room: str) -> None:
        self._subscriptions.get(ws, set()).discard((workspace, room))

    def focus(self, ws: WebSocket, workspace: str, room: str) -> None:
        self._focused[ws] = (workspace, room)

    def is_room_focused(self, workspace: str, room: str) -> bool:
        return any(
            f == (workspace, room) for f in self._focused.values() if f is not None
        )

    async def broadcast(
        self, event: dict, workspace: str, room: str
    ) -> None:
        data = json.dumps(event)
        for ws, subs in self._subscriptions.items():
            if (workspace, room) in subs:
                try:
                    await ws.send_text(data)
                except Exception:
                    pass
