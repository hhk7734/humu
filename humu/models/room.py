from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Room:
    name: str
    leader: str
    agents: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "leader": self.leader,
            "agents": self.agents,
        }

    @classmethod
    def from_dict(cls, data: dict) -> Room:
        return cls(
            name=data["name"],
            leader=data["leader"],
            agents=data.get("agents", []),
        )
