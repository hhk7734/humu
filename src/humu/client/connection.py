from __future__ import annotations

import asyncio
import json
import logging
from collections.abc import Callable
from typing import Any

import websockets

from humu.config import DEFAULT_HOST, DEFAULT_PORT
from humu.protocol import (
    FocusRoomCmd,
    SubscribeRoomCmd,
    UnsubscribeRoomCmd,
    UserMessageCmd,
)

logger = logging.getLogger(__name__)


class ServerConnection:
    def __init__(self, on_message: Callable[[dict], Any] | None = None) -> None:
        self._ws: websockets.ClientConnection | None = None
        self._on_message = on_message
        self._url = f"ws://{DEFAULT_HOST}:{DEFAULT_PORT}/ws"
        self._connected = asyncio.Event()

    async def connect(self) -> None:
        self._ws = await websockets.connect(self._url)
        self._connected.set()

    async def wait_connected(self) -> None:
        await self._connected.wait()

    async def disconnect(self) -> None:
        if self._ws:
            await self._ws.close()
            self._ws = None

    async def send(self, data: dict) -> None:
        if self._ws:
            await self._ws.send(json.dumps(data))
        else:
            logger.warning("Message dropped (not connected): %s", data.get("type"))

    async def receive_loop(self) -> None:
        if not self._ws:
            return
        try:
            async for raw in self._ws:
                if self._on_message:
                    msg = json.loads(raw)
                    self._on_message(msg)
        except websockets.exceptions.ConnectionClosed:
            pass

    async def subscribe_room(self, workspace: str, room: str) -> None:
        await self.send(
            SubscribeRoomCmd(workspace=workspace, room=room).model_dump()
        )

    async def unsubscribe_room(self, workspace: str, room: str) -> None:
        await self.send(
            UnsubscribeRoomCmd(workspace=workspace, room=room).model_dump()
        )

    async def focus_room(self, workspace: str, room: str) -> None:
        await self.send(
            FocusRoomCmd(workspace=workspace, room=room).model_dump()
        )

    async def send_message(self, workspace: str, room: str, text: str) -> None:
        await self.send(
            UserMessageCmd(workspace=workspace, room=room, text=text).model_dump()
        )
