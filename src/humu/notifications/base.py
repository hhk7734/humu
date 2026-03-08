from __future__ import annotations

from typing import Literal, Protocol, runtime_checkable

from pydantic import BaseModel


class Notification(BaseModel):
    title: str
    body: str
    room: str
    workspace: str
    severity: Literal["info", "warning", "error"]


@runtime_checkable
class NotificationProvider(Protocol):
    async def send(self, notification: Notification) -> None: ...
