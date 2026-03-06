from __future__ import annotations

import json
from pathlib import Path

from humu.config import AGENTS_DIR, HUMU_HOME, PROJECTS_DIR, WORKSPACES_FILE
from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace


class Storage:
    def __init__(self) -> None:
        HUMU_HOME.mkdir(parents=True, exist_ok=True)
        AGENTS_DIR.mkdir(parents=True, exist_ok=True)
        PROJECTS_DIR.mkdir(parents=True, exist_ok=True)
        if not WORKSPACES_FILE.exists():
            WORKSPACES_FILE.write_text("[]")

    # --- Workspaces ---

    def list_workspaces(self) -> list[Workspace]:
        data = json.loads(WORKSPACES_FILE.read_text())
        return [Workspace.from_dict(w) for w in data]

    def save_workspace(self, workspace: Workspace) -> None:
        workspaces = self.list_workspaces()
        workspaces = [w for w in workspaces if w.name != workspace.name]
        workspaces.append(workspace)
        WORKSPACES_FILE.write_text(
            json.dumps([w.to_dict() for w in workspaces], indent=2)
        )

    def delete_workspace(self, name: str) -> None:
        workspaces = [w for w in self.list_workspaces() if w.name != name]
        WORKSPACES_FILE.write_text(
            json.dumps([w.to_dict() for w in workspaces], indent=2)
        )

    def get_workspace(self, name: str) -> Workspace | None:
        for w in self.list_workspaces():
            if w.name == name:
                return w
        return None

    # --- Agents ---

    def list_agents(self) -> list[AgentConfig]:
        agents = []
        for f in AGENTS_DIR.glob("*.json"):
            data = json.loads(f.read_text())
            agents.append(AgentConfig.from_dict(data))
        return agents

    def save_agent(self, agent: AgentConfig) -> None:
        path = AGENTS_DIR / f"{agent.name}.json"
        path.write_text(json.dumps(agent.to_dict(), indent=2))

    def delete_agent(self, name: str) -> None:
        path = AGENTS_DIR / f"{name}.json"
        if path.exists():
            path.unlink()

    def get_agent(self, name: str) -> AgentConfig | None:
        path = AGENTS_DIR / f"{name}.json"
        if path.exists():
            data = json.loads(path.read_text())
            return AgentConfig.from_dict(data)
        return None

    # --- Rooms ---

    def _project_dir(self, workspace: Workspace) -> Path:
        return PROJECTS_DIR / workspace.slug

    def _rooms_dir(self, workspace: Workspace) -> Path:
        d = self._project_dir(workspace) / "rooms"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def _room_file(self, workspace: Workspace, room_name: str) -> Path:
        return self._rooms_dir(workspace) / f"{room_name}.json"

    def list_rooms(self, workspace: Workspace) -> list[Room]:
        rooms = []
        rooms_dir = self._rooms_dir(workspace)
        for f in rooms_dir.glob("*.json"):
            data = json.loads(f.read_text())
            rooms.append(Room.from_dict(data))
        return rooms

    def save_room(self, workspace: Workspace, room: Room) -> None:
        path = self._room_file(workspace, room.name)
        path.write_text(json.dumps(room.to_dict(), indent=2))

    def delete_room(self, workspace: Workspace, room_name: str) -> None:
        path = self._room_file(workspace, room_name)
        if path.exists():
            path.unlink()

    def get_room(self, workspace: Workspace, room_name: str) -> Room | None:
        path = self._room_file(workspace, room_name)
        if path.exists():
            data = json.loads(path.read_text())
            return Room.from_dict(data)
        return None

    # --- Session data ---

    def agent_room_dir(
        self, workspace: Workspace, room_name: str, agent_name: str
    ) -> Path:
        d = (
            self._project_dir(workspace)
            / "rooms"
            / room_name
            / "agents"
            / agent_name
        )
        d.mkdir(parents=True, exist_ok=True)
        return d

    def get_session_id(
        self, workspace: Workspace, room_name: str, agent_name: str
    ) -> str | None:
        d = self.agent_room_dir(workspace, room_name, agent_name)
        session_file = d / "session.json"
        if session_file.exists():
            data = json.loads(session_file.read_text())
            return data.get("session_id")
        return None

    def save_session_id(
        self,
        workspace: Workspace,
        room_name: str,
        agent_name: str,
        session_id: str,
    ) -> None:
        d = self.agent_room_dir(workspace, room_name, agent_name)
        session_file = d / "session.json"
        session_file.write_text(json.dumps({"session_id": session_id}, indent=2))

    # --- Chat history ---

    def load_chat_history(
        self, workspace: Workspace, room_name: str
    ) -> list[dict]:
        d = self._project_dir(workspace) / "rooms" / room_name
        history_file = d / "history.json"
        if history_file.exists():
            return json.loads(history_file.read_text())
        return []

    def append_chat_message(
        self, workspace: Workspace, room_name: str, message: dict
    ) -> None:
        d = self._project_dir(workspace) / "rooms" / room_name
        d.mkdir(parents=True, exist_ok=True)
        history_file = d / "history.json"
        history = []
        if history_file.exists():
            history = json.loads(history_file.read_text())
        history.append(message)
        history_file.write_text(json.dumps(history, indent=2))
