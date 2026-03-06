from __future__ import annotations

import logging
from dataclasses import dataclass, field
from typing import Any, AsyncIterator

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ClaudeSDKClient,
    ResultMessage,
    TextBlock,
)

from humu.models.agent import AgentConfig
from humu.models.workspace import Workspace
from humu.services.storage import Storage

logger = logging.getLogger(__name__)


@dataclass
class AgentResponse:
    text: str
    session_id: str | None = None


@dataclass
class StreamChunk:
    text: str
    done: bool = False


class AgentRunner:
    def __init__(self, storage: Storage) -> None:
        self._storage = storage
        self._clients: dict[str, ClaudeSDKClient] = {}

    def _client_key(self, room_name: str, agent_name: str) -> str:
        return f"{room_name}:{agent_name}"

    def _build_options(
        self,
        agent: AgentConfig,
        workspace: Workspace,
        room_name: str,
        *,
        output_format: dict | None = None,
        system_prompt_override: str | None = None,
    ) -> ClaudeAgentOptions:
        session_id = self._storage.get_session_id(
            workspace, room_name, agent.name
        )

        opts = ClaudeAgentOptions(
            system_prompt=system_prompt_override or agent.prompt,
            allowed_tools=agent.tools,
            permission_mode="bypassPermissions",
            cwd=workspace.root_path,
            model=agent.model,
            include_partial_messages=agent.streaming,
        )

        if session_id:
            opts.resume = session_id

        if output_format:
            opts.output_format = output_format

        return opts

    async def query(
        self,
        agent: AgentConfig,
        workspace: Workspace,
        room_name: str,
        prompt: str,
        *,
        output_format: dict | None = None,
        system_prompt_override: str | None = None,
    ) -> AgentResponse:
        key = self._client_key(room_name, agent.name)

        opts = self._build_options(
            agent,
            workspace,
            room_name,
            output_format=output_format,
            system_prompt_override=system_prompt_override,
        )

        try:
            client = ClaudeSDKClient(options=opts)
            self._clients[key] = client

            await client.connect()
            await client.query(prompt)

            text_parts: list[str] = []
            session_id: str | None = None
            result_text: str | None = None
            structured: Any = None

            async for message in client.receive_response():
                if isinstance(message, AssistantMessage):
                    if message.error:
                        logger.error("Agent %s error: %s", agent.name, message.error)
                    for block in message.content:
                        if isinstance(block, TextBlock):
                            text_parts.append(block.text)
                if isinstance(message, ResultMessage):
                    session_id = message.session_id
                    result_text = message.result
                    structured = message.structured_output

            await client.disconnect()
            del self._clients[key]

            if session_id:
                self._storage.save_session_id(
                    workspace, room_name, agent.name, session_id
                )

            # Prefer structured output, then result, then assembled text blocks
            if structured is not None:
                import json
                if isinstance(structured, str):
                    final_text = structured
                else:
                    final_text = json.dumps(structured)
            elif result_text:
                final_text = result_text
            else:
                final_text = "".join(text_parts)

            return AgentResponse(
                text=final_text,
                session_id=session_id,
            )
        except Exception:
            logger.exception("Agent %s query failed", agent.name)
            self._clients.pop(key, None)
            raise

    async def query_streaming(
        self,
        agent: AgentConfig,
        workspace: Workspace,
        room_name: str,
        prompt: str,
        *,
        system_prompt_override: str | None = None,
    ) -> AsyncIterator[StreamChunk]:
        key = self._client_key(room_name, agent.name)

        opts = self._build_options(
            agent,
            workspace,
            room_name,
            system_prompt_override=system_prompt_override,
        )
        opts.include_partial_messages = True

        try:
            client = ClaudeSDKClient(options=opts)
            self._clients[key] = client

            await client.connect()
            await client.query(prompt)

            session_id: str | None = None

            async for message in client.receive_response():
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, TextBlock):
                            yield StreamChunk(text=block.text)
                if isinstance(message, ResultMessage):
                    session_id = getattr(message, "session_id", None)

            if session_id:
                self._storage.save_session_id(
                    workspace, room_name, agent.name, session_id
                )

            yield StreamChunk(text="", done=True)

            await client.disconnect()
            del self._clients[key]
        except Exception:
            logger.exception("Agent %s streaming query failed", agent.name)
            self._clients.pop(key, None)
            raise

    async def disconnect_all(self) -> None:
        for key, client in list(self._clients.items()):
            try:
                await client.disconnect()
            except Exception:
                logger.exception("Failed to disconnect %s", key)
        self._clients.clear()
