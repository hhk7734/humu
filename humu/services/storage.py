from __future__ import annotations

import json
from pathlib import Path

import shutil
import subprocess

from humu.config import (
    AGENTS_DIR, HUMU_HOME, MARKETPLACES_FILE, PLUGINS_DIR, PROJECTS_DIR,
    SKILLS_CONFIG_FILE, WORKSPACES_FILE,
)

LAST_SESSION_FILE = HUMU_HOME / "last_session.json"
from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace


class Storage:
    def __init__(self) -> None:
        HUMU_HOME.mkdir(parents=True, exist_ok=True)
        PLUGINS_DIR.mkdir(parents=True, exist_ok=True)
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

    # --- Marketplaces ---

    def list_marketplaces(self) -> list[dict]:
        """Return marketplaces from ~/.humu/marketplaces.json.

        Each entry: {"id": str, "repo": str}
        """
        if not MARKETPLACES_FILE.exists():
            return []
        try:
            return json.loads(MARKETPLACES_FILE.read_text())
        except Exception:
            return []

    def add_marketplace(self, marketplace_id: str, repo: str) -> None:
        marketplaces = [m for m in self.list_marketplaces() if m["id"] != marketplace_id]
        marketplaces.append({"id": marketplace_id, "repo": repo})
        MARKETPLACES_FILE.write_text(json.dumps(marketplaces, indent=2))

    def remove_marketplace(self, marketplace_id: str) -> None:
        marketplaces = [m for m in self.list_marketplaces() if m["id"] != marketplace_id]
        MARKETPLACES_FILE.write_text(json.dumps(marketplaces, indent=2))

    # --- Plugins ---

    def plugin_dir(self, marketplace_id: str) -> Path:
        return PLUGINS_DIR / marketplace_id

    def is_plugin_installed(self, marketplace_id: str) -> bool:
        return self.plugin_dir(marketplace_id).exists()

    def install_plugin(self, marketplace_id: str, repo: str) -> tuple[bool, str]:
        """Clone the marketplace repo into ~/.humu/plugins/<marketplace_id>.

        Returns (success, message).
        """
        dest = self.plugin_dir(marketplace_id)
        if dest.exists():
            return False, f"Already installed at {dest}"
        try:
            result = subprocess.run(
                ["git", "clone", "--depth=1", f"https://github.com/{repo}.git", str(dest)],
                capture_output=True, text=True, timeout=60,
            )
            if result.returncode == 0:
                return True, f"Installed {repo}"
            return False, (result.stderr or result.stdout).strip()[:200]
        except subprocess.TimeoutExpired:
            return False, "Timed out"
        except FileNotFoundError:
            return False, "`git` not found in PATH"

    def update_plugin(self, marketplace_id: str) -> tuple[bool, str]:
        """Pull latest changes in ~/.humu/plugins/<marketplace_id>."""
        dest = self.plugin_dir(marketplace_id)
        if not dest.exists():
            return False, "Not installed"
        try:
            result = subprocess.run(
                ["git", "-C", str(dest), "pull", "--ff-only"],
                capture_output=True, text=True, timeout=60,
            )
            if result.returncode == 0:
                return True, (result.stdout or "Already up to date").strip()
            return False, (result.stderr or result.stdout).strip()[:200]
        except subprocess.TimeoutExpired:
            return False, "Timed out"
        except FileNotFoundError:
            return False, "`git` not found in PATH"

    def uninstall_plugin(self, marketplace_id: str) -> tuple[bool, str]:
        """Remove ~/.humu/plugins/<marketplace_id>."""
        dest = self.plugin_dir(marketplace_id)
        if not dest.exists():
            return False, "Not installed"
        try:
            shutil.rmtree(dest)
            return True, f"Uninstalled {marketplace_id}"
        except Exception as e:
            return False, str(e)

    # --- Skills ---

    @staticmethod
    def _parse_skill_frontmatter(content: str) -> tuple[str, str]:
        """Parse YAML frontmatter from SKILL.md. Returns (name, description)."""
        lines = content.splitlines()
        if not lines or lines[0].strip() != "---":
            return "", ""
        name = ""
        description = ""
        for line in lines[1:]:
            if line.strip() == "---":
                break
            if line.startswith("name:"):
                name = line[5:].strip()
            elif line.startswith("description:"):
                description = line[12:].strip()
        return name, description

    @staticmethod
    def _parse_skill_body(content: str) -> str:
        """Return SKILL.md content with frontmatter stripped."""
        lines = content.splitlines()
        if not lines or lines[0].strip() != "---":
            return content
        in_front = True
        body_lines = []
        for line in lines[1:]:
            if in_front and line.strip() == "---":
                in_front = False
                continue
            if not in_front:
                body_lines.append(line)
        return "\n".join(body_lines).lstrip("\n")

    # --- Skill config (enable/disable) ---

    def _load_skills_config(self) -> dict:
        if not SKILLS_CONFIG_FILE.exists():
            return {"disabled": []}
        try:
            return json.loads(SKILLS_CONFIG_FILE.read_text())
        except Exception:
            return {"disabled": []}

    def _save_skills_config(self, config: dict) -> None:
        SKILLS_CONFIG_FILE.write_text(json.dumps(config, indent=2))

    def is_skill_enabled(self, name: str) -> bool:
        return name not in set(self._load_skills_config().get("disabled", []))

    def enable_skill(self, name: str) -> None:
        config = self._load_skills_config()
        disabled = set(config.get("disabled", []))
        disabled.discard(name)
        config["disabled"] = sorted(disabled)
        self._save_skills_config(config)

    def disable_skill(self, name: str) -> None:
        config = self._load_skills_config()
        disabled = set(config.get("disabled", []))
        disabled.add(name)
        config["disabled"] = sorted(disabled)
        self._save_skills_config(config)

    def list_skills(self) -> list[dict]:
        """Return enabled skills from ~/.humu/plugins/*/skills/*/SKILL.md.

        Each entry: {"name": str, "description": str, "marketplace": str}
        """
        disabled = set(self._load_skills_config().get("disabled", []))
        seen: dict[str, dict] = {}
        for skill_md in PLUGINS_DIR.glob("*/skills/*/SKILL.md"):
            try:
                marketplace = skill_md.parts[list(skill_md.parts).index(PLUGINS_DIR.name) + 1]
                content = skill_md.read_text()
                name, description = self._parse_skill_frontmatter(content)
                if name and name not in seen:
                    seen[name] = {
                        "name": name,
                        "description": description,
                        "marketplace": marketplace,
                        "enabled": name not in disabled,
                    }
            except Exception:
                pass
        return sorted(seen.values(), key=lambda s: s["name"])

    def get_skill_content(self, name: str) -> str | None:
        """Return the body of SKILL.md (frontmatter stripped) for the named skill."""
        for skill_md in PLUGINS_DIR.glob(f"*/skills/{name}/SKILL.md"):
            try:
                return self._parse_skill_body(skill_md.read_text())
            except Exception:
                pass
        return None

    # --- Last session ---

    def _load_session_data(self) -> dict:
        if not LAST_SESSION_FILE.exists():
            return {}
        try:
            return json.loads(LAST_SESSION_FILE.read_text())
        except Exception:
            return {}

    def _save_session_data(self, data: dict) -> None:
        LAST_SESSION_FILE.write_text(json.dumps(data, indent=2))

    def save_last_session(self, workspace_name: str, room_name: str) -> None:
        data = self._load_session_data()
        data["last_workspace"] = workspace_name
        if "rooms" not in data:
            data["rooms"] = {}
        data["rooms"][workspace_name] = room_name
        self._save_session_data(data)

    def load_last_session(self) -> tuple[str, str] | None:
        data = self._load_session_data()
        workspace = data.get("last_workspace", "")
        room = data.get("rooms", {}).get(workspace, "")
        if workspace and room:
            return workspace, room
        return None

    def load_last_room(self, workspace_name: str) -> str | None:
        data = self._load_session_data()
        return data.get("rooms", {}).get(workspace_name) or None

    # --- Panel layout ---

    _DEFAULT_PANEL_WIDTHS: dict[str, int] = {
        "workspace-panel": 18,
        "room-panel": 14,
        "agent-panel": 16,
    }

    def save_panel_width(self, panel_id: str, width: int) -> None:
        data = self._load_session_data()
        data.setdefault("panel_widths", {})[panel_id] = width
        self._save_session_data(data)

    def load_panel_widths(self) -> dict[str, int]:
        data = self._load_session_data()
        saved = data.get("panel_widths", {})
        widths = dict(self._DEFAULT_PANEL_WIDTHS)
        widths.update({k: v for k, v in saved.items() if k in widths})
        return widths
