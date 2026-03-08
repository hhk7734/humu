from __future__ import annotations

from typing import Any, AsyncIterator

import openai

from humu.providers.base import LLMResponse, LLMStreamChunk, Message, Tool


class OpenAIProvider:
    def __init__(self) -> None:
        self._client: openai.AsyncOpenAI | None = None

    def _get_client(self) -> openai.AsyncOpenAI:
        if self._client is None:
            self._client = openai.AsyncOpenAI()
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
        api_messages: list[dict[str, Any]] = []
        if system_prompt:
            api_messages.append({"role": "system", "content": system_prompt})
        api_messages.extend({"role": m.role, "content": m.content} for m in messages)

        params: dict[str, Any] = {"model": model, "messages": api_messages}
        if tools:
            params["tools"] = [
                {
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                }
                for t in tools
            ]

        response = await self._get_client().chat.completions.create(**params)
        choice = response.choices[0]
        return LLMResponse(
            text=choice.message.content or "",
            usage={
                "input_tokens": response.usage.prompt_tokens if response.usage else 0,
                "output_tokens": response.usage.completion_tokens if response.usage else 0,
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
        api_messages: list[dict[str, Any]] = []
        if system_prompt:
            api_messages.append({"role": "system", "content": system_prompt})
        api_messages.extend({"role": m.role, "content": m.content} for m in messages)

        stream = await self._get_client().chat.completions.create(
            model=model, messages=api_messages, stream=True, stream_options={"include_usage": True}
        )
        async for chunk in stream:
            if chunk.choices and chunk.choices[0].delta.content:
                yield LLMStreamChunk(text=chunk.choices[0].delta.content)
            if chunk.usage:
                yield LLMStreamChunk(
                    text="",
                    done=True,
                    usage={
                        "input_tokens": chunk.usage.prompt_tokens,
                        "output_tokens": chunk.usage.completion_tokens,
                    },
                )
