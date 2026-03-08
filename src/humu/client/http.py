from __future__ import annotations

import logging

import httpx

from humu.config import DEFAULT_HOST, DEFAULT_PORT

logger = logging.getLogger(__name__)


class HttpClient:
    def __init__(self) -> None:
        self._base = f"http://{DEFAULT_HOST}:{DEFAULT_PORT}"
        self._client: httpx.AsyncClient | None = None

    async def start(self) -> None:
        self._client = httpx.AsyncClient(base_url=self._base)

    async def stop(self) -> None:
        if self._client:
            await self._client.aclose()
            self._client = None

    async def list_workspaces(self) -> list[dict]:
        resp = await self._client.get("/api/workspaces")
        resp.raise_for_status()
        return resp.json()

    async def create_workspace(self, name: str, root_path: str) -> dict:
        resp = await self._client.post(
            "/api/workspaces", json={"name": name, "root_path": root_path}
        )
        resp.raise_for_status()
        return resp.json()

    async def delete_workspace(self, name: str) -> None:
        resp = await self._client.delete(f"/api/workspaces/{name}")
        resp.raise_for_status()

    async def list_rooms(self, workspace: str) -> list[dict]:
        resp = await self._client.get(f"/api/workspaces/{workspace}/rooms")
        resp.raise_for_status()
        return resp.json()

    async def create_room(self, workspace: str, name: str) -> dict:
        resp = await self._client.post(
            f"/api/workspaces/{workspace}/rooms", json={"name": name}
        )
        resp.raise_for_status()
        return resp.json()

    async def delete_room(self, workspace: str, name: str) -> None:
        resp = await self._client.delete(
            f"/api/workspaces/{workspace}/rooms/{name}"
        )
        resp.raise_for_status()

    async def list_agents(self, workspace: str, room: str) -> list[dict]:
        resp = await self._client.get(
            f"/api/workspaces/{workspace}/rooms/{room}/agents"
        )
        resp.raise_for_status()
        return resp.json()

    async def create_agent(self, workspace: str, room: str, config: dict) -> dict:
        resp = await self._client.post(
            f"/api/workspaces/{workspace}/rooms/{room}/agents", json=config
        )
        resp.raise_for_status()
        return resp.json()

    async def update_agent(
        self, workspace: str, room: str, name: str, config: dict
    ) -> dict:
        resp = await self._client.put(
            f"/api/workspaces/{workspace}/rooms/{room}/agents/{name}", json=config
        )
        resp.raise_for_status()
        return resp.json()

    async def delete_agent(self, workspace: str, room: str, name: str) -> None:
        resp = await self._client.delete(
            f"/api/workspaces/{workspace}/rooms/{room}/agents/{name}"
        )
        resp.raise_for_status()
