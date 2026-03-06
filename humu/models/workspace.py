from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Workspace:
    name: str
    root_path: str

    @property
    def path(self) -> Path:
        return Path(self.root_path)

    @property
    def slug(self) -> str:
        return self.name.replace(" ", "-").lower()

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "root_path": self.root_path,
        }

    @classmethod
    def from_dict(cls, data: dict) -> Workspace:
        return cls(
            name=data["name"],
            root_path=data["root_path"],
        )
