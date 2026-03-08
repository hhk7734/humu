from __future__ import annotations

from typing import Any, AsyncIterator

import anthropic

from humu.providers.base import LLMResponse, LLMStreamChunk, Message, Tool


class AnthropicProvider:
    def __init__(self) -> None:
        self._client: anthropic.AsyncAnthropic | None = None

    def _get_client(self) -> anthropic.AsyncAnthropic:
        if self._client is None:
            self._client = anthropic.AsyncAnthropic()
        return self._client

    async def chat(
        self,
        messages: list[Message],
        *,
        model: str,
        system_prompt: str | None = None,
        tools: list[Tool] | None = None,
        **kwargs: Any,
    ) -> LLMResponse:
        api_messages = [{"role": m.role, "content": m.content} for m in messages]
        params: dict[str, Any] = {
            "model": model,
            "messages": api_messages,
            "max_tokens": kwargs.get("max_tokens", 8192),
        }
        if system_prompt:
            params["system"] = system_prompt
        if tools:
            params["tools"] = [
                {"name": t.name, "description": t.description, "input_schema": t.input_schema}
                for t in tools
            ]

        response = await self._get_client().messages.create(**params)
        text = "".join(
            block.text for block in response.content if block.type == "text"
        )
        return LLMResponse(
            text=text,
            usage={
                "input_tokens": response.usage.input_tokens,
                "output_tokens": response.usage.output_tokens,
            },
            raw=response,
        )

    async def chat_stream(
        self,
        messages: list[Message],
        *,
        model: str,
        system_prompt: str | None = None,
        tools: list[Tool] | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[LLMStreamChunk]:
        api_messages = [{"role": m.role, "content": m.content} for m in messages]
        params: dict[str, Any] = {
            "model": model,
            "messages": api_messages,
            "max_tokens": kwargs.get("max_tokens", 8192),
        }
        if system_prompt:
            params["system"] = system_prompt
        if tools:
            params["tools"] = [
                {"name": t.name, "description": t.description, "input_schema": t.input_schema}
                for t in tools
            ]

        async with self._get_client().messages.stream(**params) as stream:
            async for text in stream.text_stream:
                yield LLMStreamChunk(text=text)

            final = await stream.get_final_message()
            yield LLMStreamChunk(
                text="",
                done=True,
                usage={
                    "input_tokens": final.usage.input_tokens,
                    "output_tokens": final.usage.output_tokens,
                },
            )
