"""WebSocket server — manages client connections and event broadcasting."""

from __future__ import annotations

import asyncio
import json
import logging
from pathlib import Path

import websockets
import websockets.exceptions
from websockets.asyncio.server import ServerConnection, unix_serve

from humu.config import HUMU_HOME
from humu.services.agent_runner import AgentRunner
from humu.services.router import Router
from humu.services.storage import Storage
from humu.server.handler import Handler

logger = logging.getLogger(__name__)

SOCKET_PATH = HUMU_HOME / "humu.sock"
PID_FILE = HUMU_HOME / "humu.pid"


class HumuServer:
    def __init__(self) -> None:
        self._storage = Storage()
        self._runner = AgentRunner(self._storage)
        self._router = Router(self._runner, self._storage)
        self._handler = Handler(
            self._storage, self._runner, self._router, self._broadcast
        )

        # Connected clients and their room subscriptions
        self._clients: set[ServerConnection] = set()
        # ws -> set of (workspace, room) subscriptions
        self._subscriptions: dict[ServerConnection, set[tuple[str, str]]] = {}

    def _broadcast(self, event: dict, workspace: str | None, room: str | None) -> None:
        """Send event to all clients subscribed to the given workspace/room.

        If workspace is None, send to ALL connected clients (global events).
        This may be called from a non-async context (e.g. router callback),
        so we schedule the actual sends on the event loop.
        """
        data = json.dumps(event)
        targets: set[ServerConnection] = set()

        if workspace is None:
            targets = set(self._clients)
        else:
            for ws, subs in self._subscriptions.items():
                if room is None:
                    if any(w == workspace for w, _ in subs):
                        targets.add(ws)
                else:
                    if (workspace, room) in subs:
                        targets.add(ws)

        if targets:
            try:
                loop = asyncio.get_running_loop()
                loop.create_task(self._async_broadcast(data, targets))
            except RuntimeError:
                # No running loop — can't broadcast
                pass

    async def _async_broadcast(self, data: str, targets: set[ServerConnection]) -> None:
        for ws in targets:
            try:
                await ws.send(data)
            except Exception:
                pass

    async def _handle_connection(self, websocket: ServerConnection) -> None:
        self._clients.add(websocket)
        self._subscriptions[websocket] = set()
        logger.info("Client connected (%d total)", len(self._clients))

        try:
            async for raw in websocket:
                try:
                    msg = json.loads(raw)
                except (json.JSONDecodeError, TypeError):
                    await websocket.send(
                        json.dumps({"type": "error", "message": "Invalid JSON"})
                    )
                    continue

                msg_type = msg.get("type", "")

                # Handle subscriptions at server level
                if msg_type == "subscribe_room":
                    ws_name = msg.get("workspace", "")
                    room_name = msg.get("room", "")
                    if ws_name and room_name:
                        self._subscriptions[websocket].add((ws_name, room_name))
                    continue

                if msg_type == "unsubscribe_room":
                    ws_name = msg.get("workspace", "")
                    room_name = msg.get("room", "")
                    self._subscriptions[websocket].discard((ws_name, room_name))
                    continue

                # Dispatch to handler
                reply = await self._handler.handle(msg)
                if reply is not None:
                    await websocket.send(json.dumps(reply))

        except websockets.exceptions.ConnectionClosed:
            pass
        except Exception:
            logger.exception("Connection error")
        finally:
            self._clients.discard(websocket)
            self._subscriptions.pop(websocket, None)
            logger.info("Client disconnected (%d remaining)", len(self._clients))

    async def run(self) -> None:
        """Start the WebSocket server on a Unix domain socket."""
        # Clean up stale socket
        if SOCKET_PATH.exists():
            SOCKET_PATH.unlink()

        HUMU_HOME.mkdir(parents=True, exist_ok=True)

        # Write PID file
        PID_FILE.write_text(str(asyncio.get_event_loop().time()))

        logger.info("Starting Humu server on %s", SOCKET_PATH)

        async with unix_serve(
            self._handle_connection,
            path=str(SOCKET_PATH),
            max_size=64 * 1024 * 1024,  # 64 MB
        ):
            # Run forever
            await asyncio.Future()

    async def shutdown(self) -> None:
        """Clean up on shutdown."""
        if SOCKET_PATH.exists():
            SOCKET_PATH.unlink()
        if PID_FILE.exists():
            PID_FILE.unlink()
        await self._runner.disconnect_all()


def run_server() -> None:
    """Entry point for ``humu serve``."""
    logging.basicConfig(
        level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s"
    )
    server = HumuServer()

    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    try:
        loop.run_until_complete(server.run())
    except KeyboardInterrupt:
        logger.info("Shutting down...")
    finally:
        loop.run_until_complete(server.shutdown())
        loop.close()
