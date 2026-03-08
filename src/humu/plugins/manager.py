from __future__ import annotations

import json
import shutil
from pathlib import Path


class PluginManager:
    def __init__(
        self,
        marketplaces_dir: Path,
        plugins_dir: Path,
    ) -> None:
        self._marketplaces_dir = marketplaces_dir
        self._plugins_dir = plugins_dir

    def list_marketplace_plugins(self, marketplace: str) -> list[str]:
        mp_dir = self._marketplaces_dir / marketplace
        manifest = mp_dir / ".claude-plugin" / "marketplace.json"
        if not manifest.exists():
            return []
        data = json.loads(manifest.read_text())
        return data.get("plugins", [])

    def install_from_marketplace(self, marketplace: str, plugin_name: str) -> None:
        src = self._marketplaces_dir / marketplace / "plugins" / plugin_name
        if not src.exists():
            raise FileNotFoundError(f"Plugin '{plugin_name}' not found in marketplace '{marketplace}'")
        dest = self._plugins_dir / plugin_name
        if dest.exists():
            shutil.rmtree(dest)
        shutil.copytree(src, dest)

    def uninstall(self, plugin_name: str) -> None:
        dest = self._plugins_dir / plugin_name
        if dest.exists():
            shutil.rmtree(dest)

    def list_skills(self) -> list[dict]:
        results: list[dict] = []
        for skill_md in self._plugins_dir.glob("*/skills/*/SKILL.md"):
            rel = skill_md.relative_to(self._plugins_dir)
            plugin_name = rel.parts[0]
            skill_dir_name = skill_md.parent.name
            full_name = f"{plugin_name}:{skill_dir_name}"
            _, description = self._parse_frontmatter(skill_md.read_text())
            results.append({
                "name": full_name,
                "plugin": plugin_name,
                "description": description,
            })
        return sorted(results, key=lambda s: s["name"])

    def get_skill_content(self, name: str) -> str | None:
        if ":" not in name:
            return None
        plugin_name, skill_dir_name = name.split(":", 1)
        skill_md = self._plugins_dir / plugin_name / "skills" / skill_dir_name / "SKILL.md"
        if not skill_md.exists():
            return None
        return self._strip_frontmatter(skill_md.read_text())

    @staticmethod
    def _parse_frontmatter(content: str) -> tuple[str, str]:
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
    def _strip_frontmatter(content: str) -> str:
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
