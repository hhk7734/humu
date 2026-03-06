from __future__ import annotations

import json
import logging
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, AsyncIterator

from humu.config import ROUTING_SCHEMA
from humu.models.agent import AgentConfig
from humu.models.room import Room
from humu.models.workspace import Workspace
from humu.services.agent_runner import AgentRunner, StreamChunk
from humu.services.storage import Storage

if TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


@dataclass
class ChatMessage:
    sender: str
    text: str
    is_system: bool = False
    raw: str | None = None
    is_loading: bool = False
    steps: list[dict] = field(default_factory=list)


class Router:
    def __init__(self, runner: AgentRunner, storage: Storage) -> None:
        self._runner = runner
        self._storage = storage

    def _build_leader_prompt(self, leader: AgentConfig, room: Room) -> str:
        agent_descriptions = []
        for agent_name in room.agents:
            agent = self._storage.get_agent(agent_name)
            if agent:
                agent_descriptions.append(
                    f"- **{agent.name}**: {agent.description}"
                )

        agents_section = "\n".join(agent_descriptions) if agent_descriptions else "No member agents available."

        return f"""{leader.prompt}

You are the leader agent of room "{room.name}". When you receive a user message, decide how to handle it.

Available member agents:
{agents_section}

You MUST respond with a JSON object with one of these formats:
- Direct answer: {{"action": "direct", "message": "your response"}}
- Forward to agent(s): {{"action": "forward", "targets": ["agent-name"], "context": "context for the agent"}}
- Chain agents: {{"action": "chain", "steps": [{{"agent": "agent-name", "context": "context"}}]}}

If no member agents are available or the question doesn't need specialist help, answer directly.
When forwarding, include enough context in the "context" field for the agent to understand what's needed."""

    async def handle_message(
        self,
        workspace: Workspace,
        room: Room,
        user_message: str,
    ) -> AsyncIterator[ChatMessage]:
        leader = self._storage.get_agent(room.leader)
        if not leader:
            yield ChatMessage(
                sender="system",
                text=f"Leader agent '{room.leader}' not found.",
                is_system=True,
            )
            return

        leader_prompt = self._build_leader_prompt(leader, room)

        yield ChatMessage(sender=room.leader, text="", is_loading=True)
        try:
            response = await self._runner.query(
                leader,
                workspace,
                room.name,
                user_message,
                output_format={"type": "json_schema", "schema": ROUTING_SCHEMA},
                system_prompt_override=leader_prompt,
            )
        except Exception as e:
            yield ChatMessage(
                sender="error",
                text=f"Leader agent error: {e}",
                is_system=True,
            )
            return

        raw_response = response.text

        try:
            decision = json.loads(response.text)
        except (json.JSONDecodeError, TypeError):
            yield ChatMessage(sender=room.leader, text=response.text, raw=raw_response)
            return

        action = decision.get("action", "direct")

        if action == "direct":
            yield ChatMessage(
                sender=room.leader,
                text=decision.get("message", response.text),
                raw=raw_response,
                steps=response.steps,
            )

        elif action == "forward":
            targets = decision.get("targets", [])
            context = decision.get("context", user_message)

            if not targets:
                yield ChatMessage(sender=room.leader, text=response.text)
                return

            yield ChatMessage(
                sender=room.leader,
                text=f"Forwarding to {', '.join(targets)}...",
                is_system=True,
            )

            agent_responses: list[tuple[str, str]] = []

            for target_name in targets:
                if target_name not in room.agents:
                    yield ChatMessage(
                        sender="error",
                        text=f"Agent '{target_name}' is not in this room.",
                        is_system=True,
                    )
                    continue

                agent = self._storage.get_agent(target_name)
                if not agent:
                    yield ChatMessage(
                        sender="error",
                        text=f"Agent '{target_name}' not found.",
                        is_system=True,
                    )
                    continue

                forward_prompt = f"The leader agent forwarded the following to you.\n\nOriginal user message: {user_message}\n\nLeader's context: {context}"

                yield ChatMessage(sender=target_name, text="", is_loading=True)
                if agent.streaming:
                    text_parts: list[str] = []
                    streaming_steps: list[dict] = []
                    async for chunk in self._runner.query_streaming(
                        agent, workspace, room.name, forward_prompt
                    ):
                        if chunk.done:
                            streaming_steps = chunk.steps
                        else:
                            text_parts.append(chunk.text)
                            yield ChatMessage(sender=target_name, text=chunk.text)
                    full_text = "".join(text_parts)
                    if streaming_steps:
                        yield ChatMessage(
                            sender=target_name,
                            text="Process log (right-click for details)",
                            is_system=True,
                            steps=streaming_steps,
                        )
                else:
                    try:
                        agent_resp = await self._runner.query(
                            agent, workspace, room.name, forward_prompt
                        )
                        yield ChatMessage(
                            sender=target_name,
                            text=agent_resp.text,
                            raw=agent_resp.text,
                            steps=agent_resp.steps,
                        )
                        full_text = agent_resp.text
                    except Exception as e:
                        yield ChatMessage(
                            sender="error",
                            text=f"Agent '{target_name}' error: {e}",
                            is_system=True,
                        )
                        continue

                agent_responses.append((target_name, full_text))

            if agent_responses:
                summary_parts = [
                    f"[{name}]: {text}" for name, text in agent_responses
                ]
                synthesis_prompt = (
                    f"The user asked: {user_message}\n\n"
                    f"Here are the responses from the agents you forwarded to:\n\n"
                    + "\n\n".join(summary_parts)
                    + "\n\nPlease synthesize these responses into a coherent answer for the user."
                )

                yield ChatMessage(sender=room.leader, text="", is_loading=True)
                try:
                    synthesis = await self._runner.query(
                        leader,
                        workspace,
                        room.name,
                        synthesis_prompt,
                    )
                    yield ChatMessage(
                        sender=room.leader,
                        text=synthesis.text,
                        raw=synthesis.text,
                        steps=synthesis.steps,
                    )
                except Exception as e:
                    yield ChatMessage(
                        sender="error",
                        text=f"Leader synthesis error: {e}",
                        is_system=True,
                    )

        elif action == "chain":
            steps = decision.get("steps", [])
            if not steps:
                yield ChatMessage(sender=room.leader, text=response.text)
                return

            yield ChatMessage(
                sender=room.leader,
                text=f"Chaining through {', '.join(s['agent'] for s in steps)}...",
                is_system=True,
            )

            previous_output = ""
            agent_results: list[tuple[str, str]] = []

            for step in steps:
                agent_name = step["agent"]
                step_context = step.get("context", "")

                if agent_name not in room.agents:
                    yield ChatMessage(
                        sender="error",
                        text=f"Agent '{agent_name}' is not in this room.",
                        is_system=True,
                    )
                    continue

                agent = self._storage.get_agent(agent_name)
                if not agent:
                    yield ChatMessage(
                        sender="error",
                        text=f"Agent '{agent_name}' not found.",
                        is_system=True,
                    )
                    continue

                chain_prompt = (
                    f"The leader agent forwarded the following to you.\n\n"
                    f"Original user message: {user_message}\n\n"
                    f"Leader's context: {step_context}"
                )
                if previous_output:
                    chain_prompt += (
                        f"\n\nOutput from previous agent:\n{previous_output}"
                    )

                yield ChatMessage(sender=agent_name, text="", is_loading=True)
                if agent.streaming:
                    text_parts_chain: list[str] = []
                    chain_steps: list[dict] = []
                    async for chunk in self._runner.query_streaming(
                        agent, workspace, room.name, chain_prompt
                    ):
                        if chunk.done:
                            chain_steps = chunk.steps
                        else:
                            text_parts_chain.append(chunk.text)
                            yield ChatMessage(sender=agent_name, text=chunk.text)
                    previous_output = "".join(text_parts_chain)
                    if chain_steps:
                        yield ChatMessage(
                            sender=agent_name,
                            text="Process log (right-click for details)",
                            is_system=True,
                            steps=chain_steps,
                        )
                else:
                    try:
                        agent_resp = await self._runner.query(
                            agent, workspace, room.name, chain_prompt
                        )
                        yield ChatMessage(
                            sender=agent_name,
                            text=agent_resp.text,
                            raw=agent_resp.text,
                            steps=agent_resp.steps,
                        )
                        previous_output = agent_resp.text
                    except Exception as e:
                        yield ChatMessage(
                            sender="error",
                            text=f"Agent '{agent_name}' error: {e}",
                            is_system=True,
                        )
                        previous_output = f"Error: {e}"

                agent_results.append((agent_name, previous_output))

            if agent_results:
                summary_parts = [
                    f"[{name}]: {text}" for name, text in agent_results
                ]
                synthesis_prompt = (
                    f"The user asked: {user_message}\n\n"
                    f"Here are the chained responses from agents:\n\n"
                    + "\n\n".join(summary_parts)
                    + "\n\nPlease synthesize these into a final coherent answer for the user."
                )

                yield ChatMessage(sender=room.leader, text="", is_loading=True)
                try:
                    synthesis = await self._runner.query(
                        leader,
                        workspace,
                        room.name,
                        synthesis_prompt,
                    )
                    yield ChatMessage(
                        sender=room.leader,
                        text=synthesis.text,
                        raw=synthesis.text,
                        steps=synthesis.steps,
                    )
                except Exception as e:
                    yield ChatMessage(
                        sender="error",
                        text=f"Leader synthesis error: {e}",
                        is_system=True,
                    )
        else:
            yield ChatMessage(sender=room.leader, text=response.text)
