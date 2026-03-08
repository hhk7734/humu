from __future__ import annotations

from typing import Literal

from pydantic import BaseModel


# --- Client -> Server ---


class UserMessageCmd(BaseModel):
    type: Literal["user_message"] = "user_message"
    workspace: str
    room: str
    text: str


class SubscribeRoomCmd(BaseModel):
    type: Literal["subscribe_room"] = "subscribe_room"
    workspace: str
    room: str


class UnsubscribeRoomCmd(BaseModel):
    type: Literal["unsubscribe_room"] = "unsubscribe_room"
    workspace: str
    room: str


class FocusRoomCmd(BaseModel):
    type: Literal["focus_room"] = "focus_room"
    workspace: str
    room: str


class ClientMessage:
    _registry: dict[str, type[BaseModel]] = {
        "user_message": UserMessageCmd,
        "subscribe_room": SubscribeRoomCmd,
        "unsubscribe_room": UnsubscribeRoomCmd,
        "focus_room": FocusRoomCmd,
    }

    @classmethod
    def parse(cls, data: dict) -> BaseModel:
        msg_type = data.get("type", "")
        model = cls._registry.get(msg_type)
        if model is None:
            raise ValueError(f"Unknown message type: {msg_type}")
        return model.model_validate(data)


# --- Server -> Client ---


class StreamChunkEvent(BaseModel):
    type: Literal["stream_chunk"] = "stream_chunk"
    workspace: str
    room: str
    sender: str
    text: str


class AgentStatusEvent(BaseModel):
    type: Literal["agent_status"] = "agent_status"
    workspace: str
    room: str
    agent: str
    status: Literal["started", "completed", "error"]
    error: str | None = None


class DagUpdateEvent(BaseModel):
    type: Literal["dag_update"] = "dag_update"
    workspace: str
    room: str
    nodes: list[dict]  # [{name, status: "running"|"done"|"pending"}]


class RoomStateSyncEvent(BaseModel):
    type: Literal["room_state_sync"] = "room_state_sync"
    workspace: str
    room: str
    messages: list[dict]


class MessageAddedEvent(BaseModel):
    type: Literal["message_added"] = "message_added"
    workspace: str
    room: str
    sender: str
    text: str
    is_system: bool = False
    steps: list[dict] = []


class ErrorEvent(BaseModel):
    type: Literal["error"] = "error"
    message: str


class ServerMessage:
    @staticmethod
    def stream_chunk(workspace: str, room: str, sender: str, text: str) -> dict:
        return StreamChunkEvent(
            workspace=workspace, room=room, sender=sender, text=text
        ).model_dump()

    @staticmethod
    def agent_status(
        workspace: str,
        room: str,
        agent: str,
        status: Literal["started", "completed", "error"],
        error: str | None = None,
    ) -> dict:
        return AgentStatusEvent(
            workspace=workspace, room=room, agent=agent, status=status, error=error
        ).model_dump()

    @staticmethod
    def message_added(
        workspace: str, room: str, sender: str, text: str, **kwargs
    ) -> dict:
        return MessageAddedEvent(
            workspace=workspace, room=room, sender=sender, text=text, **kwargs
        ).model_dump()

    @staticmethod
    def room_state_sync(workspace: str, room: str, messages: list[dict]) -> dict:
        return RoomStateSyncEvent(
            workspace=workspace, room=room, messages=messages
        ).model_dump()

    @staticmethod
    def error(message: str) -> dict:
        return ErrorEvent(message=message).model_dump()
