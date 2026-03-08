from __future__ import annotations

from fastapi import APIRouter, Depends, Request, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

from humu.db.repositories import Repository
from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.server.ws import WebSocketManager


class CreateWorkspaceRequest(BaseModel):
    name: str
    root_path: str


class CreateRoomRequest(BaseModel):
    name: str
    leader: str


def get_repo(request: Request) -> Repository:
    return request.app.state.repo


def get_ws_manager(request: Request) -> WebSocketManager:
    return request.app.state.ws_manager


def create_router() -> APIRouter:
    router = APIRouter()

    @router.get("/health")
    async def health():
        return {"status": "ok"}

    # --- Workspaces ---

    @router.get("/api/workspaces")
    async def list_workspaces(repo: Repository = Depends(get_repo)):
        return [ws.model_dump() for ws in await repo.list_workspaces()]

    @router.post("/api/workspaces", status_code=201)
    async def create_workspace(
        body: CreateWorkspaceRequest, repo: Repository = Depends(get_repo)
    ):
        ws = Workspace(name=body.name, root_path=body.root_path)
        await repo.save_workspace(ws)
        return ws.model_dump()

    @router.delete("/api/workspaces/{name}")
    async def delete_workspace(name: str, repo: Repository = Depends(get_repo)):
        await repo.delete_workspace(name)
        return {"ok": True}

    # --- Rooms ---

    @router.get("/api/workspaces/{workspace}/rooms")
    async def list_rooms(workspace: str, repo: Repository = Depends(get_repo)):
        return [r.model_dump() for r in await repo.list_rooms(workspace)]

    @router.post("/api/workspaces/{workspace}/rooms", status_code=201)
    async def create_room(
        workspace: str,
        body: CreateRoomRequest,
        repo: Repository = Depends(get_repo),
    ):
        room = Room(name=body.name, leader=body.leader)
        await repo.save_room(workspace, room)
        return room.model_dump()

    # --- Agents ---

    @router.get("/api/workspaces/{workspace}/agents")
    async def list_agents(workspace: str, repo: Repository = Depends(get_repo)):
        return [a.model_dump() for a in await repo.list_agents(workspace)]

    @router.post("/api/workspaces/{workspace}/agents", status_code=201)
    async def create_agent(
        workspace: str, body: AgentConfig, repo: Repository = Depends(get_repo)
    ):
        await repo.save_agent(workspace, body)
        return body.model_dump()

    # --- WebSocket ---

    @router.websocket("/ws")
    async def websocket_endpoint(websocket: WebSocket):
        manager: WebSocketManager = websocket.app.state.ws_manager
        repo: Repository = websocket.app.state.repo

        await manager.connect(websocket)
        try:
            while True:
                data = await websocket.receive_json()
                msg_type = data.get("type", "")

                if msg_type == "subscribe_room":
                    manager.subscribe(
                        websocket, data["workspace"], data["room"]
                    )
                    messages = await repo.get_messages(
                        data["workspace"], data["room"]
                    )
                    from humu.protocol import ServerMessage

                    await websocket.send_json(
                        ServerMessage.room_state_sync(
                            data["workspace"], data["room"], messages
                        )
                    )

                elif msg_type == "unsubscribe_room":
                    manager.unsubscribe(
                        websocket, data["workspace"], data["room"]
                    )

                elif msg_type == "focus_room":
                    manager.focus(
                        websocket, data["workspace"], data["room"]
                    )

                elif msg_type == "user_message":
                    # TODO: Task 8 will implement full LangGraph execution
                    pass

        except WebSocketDisconnect:
            pass
        finally:
            manager.disconnect(websocket)

    return router
