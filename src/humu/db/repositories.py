import json

from humu.db.database import Database
from humu.models.workspace import Workspace
from humu.models.room import Room
from humu.models.agent import AgentConfig


class Repository:
    def __init__(self, db: Database) -> None:
        self._db = db

    # --- Workspaces ---

    async def list_workspaces(self) -> list[Workspace]:
        cursor = await self._db.conn.execute(
            "SELECT name, root_path FROM workspaces"
        )
        rows = await cursor.fetchall()
        return [Workspace(name=r["name"], root_path=r["root_path"]) for r in rows]

    async def get_workspace(self, name: str) -> Workspace | None:
        cursor = await self._db.conn.execute(
            "SELECT name, root_path FROM workspaces WHERE name = ?", (name,)
        )
        row = await cursor.fetchone()
        if row:
            return Workspace(name=row["name"], root_path=row["root_path"])
        return None

    async def save_workspace(self, ws: Workspace) -> None:
        await self._db.conn.execute(
            "INSERT OR REPLACE INTO workspaces (name, root_path) VALUES (?, ?)",
            (ws.name, ws.root_path),
        )
        await self._db.conn.commit()

    async def delete_workspace(self, name: str) -> None:
        await self._db.conn.execute(
            "DELETE FROM workspaces WHERE name = ?", (name,)
        )
        await self._db.conn.commit()

    # --- Rooms ---

    async def list_rooms(self, workspace: str) -> list[Room]:
        cursor = await self._db.conn.execute(
            "SELECT name, leader, agents FROM rooms WHERE workspace = ?",
            (workspace,),
        )
        rows = await cursor.fetchall()
        return [
            Room(
                name=r["name"],
                leader=r["leader"],
                agents=json.loads(r["agents"]),
            )
            for r in rows
        ]

    async def get_room(self, workspace: str, name: str) -> Room | None:
        cursor = await self._db.conn.execute(
            "SELECT name, leader, agents FROM rooms WHERE workspace = ? AND name = ?",
            (workspace, name),
        )
        row = await cursor.fetchone()
        if row:
            return Room(
                name=row["name"],
                leader=row["leader"],
                agents=json.loads(row["agents"]),
            )
        return None

    async def save_room(self, workspace: str, room: Room) -> None:
        await self._db.conn.execute(
            "INSERT OR REPLACE INTO rooms (workspace, name, leader, agents) VALUES (?, ?, ?, ?)",
            (workspace, room.name, room.leader, json.dumps(room.agents)),
        )
        await self._db.conn.commit()

    async def delete_room(self, workspace: str, name: str) -> None:
        await self._db.conn.execute(
            "DELETE FROM rooms WHERE workspace = ? AND name = ?",
            (workspace, name),
        )
        await self._db.conn.commit()

    # --- Agents ---

    async def list_agents(self, workspace: str) -> list[AgentConfig]:
        cursor = await self._db.conn.execute(
            "SELECT config FROM agents WHERE workspace = ?", (workspace,)
        )
        rows = await cursor.fetchall()
        return [AgentConfig.model_validate_json(r["config"]) for r in rows]

    async def get_agent(self, workspace: str, name: str) -> AgentConfig | None:
        cursor = await self._db.conn.execute(
            "SELECT config FROM agents WHERE workspace = ? AND name = ?",
            (workspace, name),
        )
        row = await cursor.fetchone()
        if row:
            return AgentConfig.model_validate_json(row["config"])
        return None

    async def save_agent(self, workspace: str, agent: AgentConfig) -> None:
        await self._db.conn.execute(
            "INSERT OR REPLACE INTO agents (workspace, name, config) VALUES (?, ?, ?)",
            (workspace, agent.name, agent.model_dump_json()),
        )
        await self._db.conn.commit()

    async def delete_agent(self, workspace: str, name: str) -> None:
        await self._db.conn.execute(
            "DELETE FROM agents WHERE workspace = ? AND name = ?",
            (workspace, name),
        )
        await self._db.conn.commit()

    # --- Messages ---

    async def append_message(self, workspace: str, room: str, data: dict) -> None:
        await self._db.conn.execute(
            "INSERT INTO messages (workspace, room, data) VALUES (?, ?, ?)",
            (workspace, room, json.dumps(data)),
        )
        await self._db.conn.commit()

    async def get_messages(self, workspace: str, room: str) -> list[dict]:
        cursor = await self._db.conn.execute(
            "SELECT data FROM messages WHERE workspace = ? AND room = ? ORDER BY id",
            (workspace, room),
        )
        rows = await cursor.fetchall()
        return [json.loads(r["data"]) for r in rows]
