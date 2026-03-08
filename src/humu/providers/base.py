from __future__ import annotations

from typing import Any, AsyncIterator, Protocol, runtime_checkable

from pydantic import BaseModel, Field


class Message(BaseModel):
    role: str  # "system", "user", "assistant"
    content: str


class Tool(BaseModel):
    name: str
    description: str
    input_schema: dict = Field(default_factory=dict)


class LLMResponse(BaseModel):
    text: str
    usage: dict = Field(default_factory=dict)
    raw: Any = None


class LLMStreamChunk(BaseModel):
    text: str
    done: bool = False
    usage: dict = Field(default_factory=dict)


@runtime_checkable
class LLMProvider(Protocol):
    async def chat(
        self,
        messages: list[Message],
        *,
        model: str,
        system_prompt: str | None = None,
        tools: list[Tool] | None = None,
        **kwargs: Any,
    ) -> LLMResponse: ...

    async def chat_stream(
        self,
        messages: list[Message],
        *,
        model: str,
        system_prompt: str | None = None,
        tools: list[Tool] | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[LLMStreamChunk]: ...
