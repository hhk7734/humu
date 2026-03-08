from __future__ import annotations

from dataclasses import dataclass, field

from humu.config import DEFAULT_MODEL, DEFAULT_TOOLS


@dataclass
class AgentConfig:
    name: str
    description: str
    prompt: str
    model: str = DEFAULT_MODEL
    tools: list[str] = field(default_factory=lambda: list(DEFAULT_TOOLS))
    streaming: bool = False

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "description": self.description,
            "prompt": self.prompt,
            "model": self.model,
            "tools": self.tools,
            "streaming": self.streaming,
        }

    @classmethod
    def from_dict(cls, data: dict) -> AgentConfig:
        return cls(
            name=data["name"],
            description=data["description"],
            prompt=data["prompt"],
            model=data.get("model", DEFAULT_MODEL),
            tools=data.get("tools", list(DEFAULT_TOOLS)),
            streaming=data.get("streaming", False),
        )
