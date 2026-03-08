from __future__ import annotations

from pydantic import BaseModel, Field


class ChatMessage(BaseModel):
    workspace: str
    room: str
    sender: str
    text: str
    is_system: bool = False
    steps: list[dict] = Field(default_factory=list)
