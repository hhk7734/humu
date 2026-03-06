from __future__ import annotations

import logging
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, AsyncIterator

from claude_agent_sdk import (
    AssistantMessage,
    ClaudeAgentOptions,
    ClaudeSDKClient,
    ResultMessage,
    SystemMessage,
    TaskProgressMessage,
    TextBlock,
    ThinkingBlock,
    ToolResultBlock,
    ToolUseBlock,
)

from humu.models.agent import AgentConfig
from humu.models.workspace import Workspace
from humu.services.storage import Storage

logger = logging.getLogger(__name__)


@dataclass
class AgentResponse:
    text: str
    session_id: str | None = None
    steps: list[dict] = field(default_factory=list)
    usage: dict | None = None


@dataclass
class StreamChunk:
    text: str
    done: bool = False
    steps: list[dict] = field(default_factory=list)
    usage: dict | None = None


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
        session_id = self._storage.get_session_id(workspace, room_name, agent.name)

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
        step_callback: Callable[[dict], None] | None = None,
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
            result_usage: dict | None = None
            steps: list[dict] = []

            async for message in client.receive_response():
                if isinstance(message, SystemMessage):
                    s: dict = {
                        "type": "system",
                        "subtype": message.subtype,
                        "data": message.data,
                    }
                    steps.append(s)
                    if step_callback:
                        step_callback(s)
                if isinstance(message, TaskProgressMessage):
                    step: dict = {
                        "type": "task_progress",
                        "description": message.description,
                    }
                    if message.last_tool_name:
                        step["tool"] = message.last_tool_name
                    usage_raw = message.usage
                    logger.debug(
                        "Agent %s TaskProgress usage raw: %r (type=%s)",
                        agent.name,
                        usage_raw,
                        type(usage_raw).__name__,
                    )
                    if usage_raw:
                        total = (
                            usage_raw.get("total_tokens", 0)
                            if isinstance(usage_raw, dict)
                            else getattr(usage_raw, "total_tokens", 0)
                        )
                        step["usage"] = {"total_tokens": total}
                    steps.append(step)
                    if step_callback:
                        step_callback(step)
                if isinstance(message, AssistantMessage):
                    if message.error:
                        logger.error("Agent %s error: %s", agent.name, message.error)
                    for block in message.content:
                        if isinstance(block, ThinkingBlock):
                            s = {"type": "thinking", "content": block.thinking}
                            steps.append(s)
                            if step_callback:
                                step_callback(s)
                        elif isinstance(block, ToolUseBlock):
                            s = {
                                "type": "tool_use",
                                "id": block.id,
                                "name": block.name,
                                "input": block.input,
                            }
                            steps.append(s)
                            if step_callback:
                                step_callback(s)
                        elif isinstance(block, ToolResultBlock):
                            content = block.content
                            if isinstance(content, list):
                                content = "\n".join(
                                    c.get("text", str(c))
                                    if isinstance(c, dict)
                                    else str(c)
                                    for c in content
                                )
                            s = {
                                "type": "tool_result",
                                "tool_use_id": block.tool_use_id,
                                "content": content or "",
                                "is_error": bool(block.is_error),
                            }
                            steps.append(s)
                            if step_callback:
                                step_callback(s)
                        elif isinstance(block, TextBlock):
                            text_parts.append(block.text)
                if isinstance(message, ResultMessage):
                    session_id = message.session_id
                    result_text = message.result
                    structured = message.structured_output
                    result_usage = message.usage
                    logger.debug(
                        "Agent %s ResultMessage usage: %r", agent.name, result_usage
                    )

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
                steps=steps,
                usage=result_usage,
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
        step_callback: Callable[[dict], None] | None = None,
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
            result_usage: dict | None = None
            steps: list[dict] = []

            async for message in client.receive_response():
                if isinstance(message, SystemMessage):
                    s_sys: dict = {
                        "type": "system",
                        "subtype": message.subtype,
                        "data": message.data,
                    }
                    steps.append(s_sys)
                    if step_callback:
                        step_callback(s_sys)
                if isinstance(message, TaskProgressMessage):
                    s2: dict = {
                        "type": "task_progress",
                        "description": message.description,
                    }
                    if message.last_tool_name:
                        s2["tool"] = message.last_tool_name
                    usage_raw2 = message.usage
                    logger.debug(
                        "Agent %s streaming TaskProgress usage raw: %r (type=%s)",
                        agent.name,
                        usage_raw2,
                        type(usage_raw2).__name__,
                    )
                    if usage_raw2:
                        total2 = (
                            usage_raw2.get("total_tokens", 0)
                            if isinstance(usage_raw2, dict)
                            else getattr(usage_raw2, "total_tokens", 0)
                        )
                        s2["usage"] = {"total_tokens": total2}
                    steps.append(s2)
                    if step_callback:
                        step_callback(s2)
                if isinstance(message, AssistantMessage):
                    for block in message.content:
                        if isinstance(block, ThinkingBlock):
                            s2 = {"type": "thinking", "content": block.thinking}
                            steps.append(s2)
                            if step_callback:
                                step_callback(s2)
                        elif isinstance(block, ToolUseBlock):
                            s2 = {
                                "type": "tool_use",
                                "id": block.id,
                                "name": block.name,
                                "input": block.input,
                            }
                            steps.append(s2)
                            if step_callback:
                                step_callback(s2)
                        elif isinstance(block, ToolResultBlock):
                            content = block.content
                            if isinstance(content, list):
                                content = "\n".join(
                                    c.get("text", str(c))
                                    if isinstance(c, dict)
                                    else str(c)
                                    for c in content
                                )
                            s2 = {
                                "type": "tool_result",
                                "tool_use_id": block.tool_use_id,
                                "content": content or "",
                                "is_error": bool(block.is_error),
                            }
                            steps.append(s2)
                            if step_callback:
                                step_callback(s2)
                        elif isinstance(block, TextBlock):
                            yield StreamChunk(text=block.text)
                if isinstance(message, ResultMessage):
                    session_id = getattr(message, "session_id", None)
                    result_usage = message.usage
                    logger.debug(
                        "Agent %s streaming ResultMessage usage: %r",
                        agent.name,
                        result_usage,
                    )

            if session_id:
                self._storage.save_session_id(
                    workspace, room_name, agent.name, session_id
                )

            yield StreamChunk(text="", done=True, steps=steps, usage=result_usage)

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
