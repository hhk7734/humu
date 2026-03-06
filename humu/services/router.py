from __future__ import annotations

import json
import logging
import threading
from dataclasses import dataclass, field
from collections.abc import Callable
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
        self._live_steps: dict[tuple[str, str], list[dict]] = {}
        self._live_steps_lock = threading.Lock()
        # Callback for system events: (room_key, agent_name, step_data) -> None
        self.on_system_event: Callable[[tuple[str, str], str, dict], None] | None = None
        # Track latest token usage per agent: (ws_name, room_name, agent_name) -> total_tokens
        self._agent_tokens: dict[tuple[str, str, str], int] = {}
        self._agent_tokens_lock = threading.Lock()

    def _add_live_step(self, room_key: tuple[str, str], step: dict, agent_name: str = "") -> None:
        with self._live_steps_lock:
            self._live_steps.setdefault(room_key, []).append(step)
        # Track token usage from task_progress steps
        if step.get("type") == "task_progress" and "usage" in step and agent_name:
            total = step["usage"].get("total_tokens", 0)
            if total > 0:
                with self._agent_tokens_lock:
                    self._agent_tokens[(*room_key, agent_name)] = total
        # Detect system events (including compaction)
        if step.get("type") == "system" and self.on_system_event:
            self.on_system_event(room_key, agent_name, step)

    def get_agent_tokens(self, ws_name: str, room_name: str, agent_name: str) -> int:
        """Return the latest known total_tokens for an agent, or 0."""
        with self._agent_tokens_lock:
            return self._agent_tokens.get((ws_name, room_name, agent_name), 0)

    def clear_agent_tokens(self, ws_name: str, room_name: str) -> None:
        """Remove all cached token counts for a room."""
        with self._agent_tokens_lock:
            keys_to_remove = [
                k for k in self._agent_tokens if k[0] == ws_name and k[1] == room_name
            ]
            for k in keys_to_remove:
                del self._agent_tokens[k]

    def _update_tokens_from_result(self, room_key: tuple[str, str], agent_name: str, usage: dict | None) -> None:
        """Extract token count from ResultMessage usage and store it."""
        if not usage or not agent_name:
            return
        # Try common key names for total input tokens
        total = (
            usage.get("total_tokens")
            or usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
            or usage.get("totalTokens")
            or 0
        )
        if total and total > 0:
            with self._agent_tokens_lock:
                self._agent_tokens[(*room_key, agent_name)] = total

    def get_live_steps(self, room_key: tuple[str, str]) -> list[dict]:
        with self._live_steps_lock:
            return list(self._live_steps.get(room_key, []))

    def _clear_live_steps(self, room_key: tuple[str, str]) -> None:
        with self._live_steps_lock:
            self._live_steps.pop(room_key, None)

    def _build_skill_context(self) -> str:
        """Build the Available Skills section from installed plugins."""
        skills = self._storage.list_skills()
        if not skills:
            return ""
        lines = ["## Available Skills", "Use these skills automatically when the user's request matches:", ""]
        for s in skills:
            if not s.get("enabled", True):
                continue
            lines.append(f"- **{s['name']}**: {s['description']}")
        return "\n".join(lines)

    def _extract_skill(self, user_message: str) -> tuple[str | None, str, str | None]:
        """Detect /skill-name prefix in user message.

        Returns (skill_name, cleaned_message, skill_body).
        """
        stripped = user_message.strip()
        if stripped.startswith("/"):
            parts = stripped.split(None, 1)
            skill_name = parts[0][1:]
            remaining = parts[1] if len(parts) > 1 else ""
            skill_body = self._storage.get_skill_content(skill_name)
            if skill_body:
                return skill_name, remaining, skill_body
        return None, user_message, None

    def _build_leader_prompt(
        self,
        leader: AgentConfig,
        room: Room,
        skill_context: str = "",
        skill_name: str | None = None,
        skill_body: str | None = None,
        is_new_session: bool = True,
    ) -> str:
        agent_descriptions = []
        for agent_name in room.agents:
            agent = self._storage.get_agent(agent_name)
            if agent:
                agent_descriptions.append(
                    f"- **{agent.name}**: {agent.description}"
                )

        agents_section = "\n".join(agent_descriptions) if agent_descriptions else "No member agents available."

        # Skill descriptions are injected only once — when the session is first created
        skill_section = f"\n{skill_context}\n" if (is_new_session and skill_context) else ""

        prompt = f"""{leader.prompt}
{skill_section}
You are the leader agent of room "{room.name}". When you receive a user message, decide how to handle it.

Available member agents:
{agents_section}

You MUST respond with a JSON object with one of these formats:
- Direct answer: {{"action": "direct", "message": "your response"}}
- Forward to agent(s): {{"action": "forward", "targets": ["agent-name"], "context": "context for the agent"}}
- Chain agents: {{"action": "chain", "steps": [{{"agent": "agent-name", "context": "context"}}]}}

If no member agents are available or the question doesn't need specialist help, answer directly.
When forwarding, include enough context in the "context" field for the agent to understand what's needed."""

        # Active skill body is always injected (message-specific, not session-level)
        if skill_name and skill_body:
            prompt += f"\n\n## Active Skill: {skill_name}\n{skill_body}"

        return prompt

    def _build_agent_prompt(
        self,
        agent: AgentConfig,
        skill_context: str = "",
        skill_name: str | None = None,
        skill_body: str | None = None,
        is_new_session: bool = True,
    ) -> str:
        """Build system prompt for a member agent.

        Skill descriptions are only injected when starting a new session.
        Active skill body is always injected (it is message-specific).
        """
        prompt = agent.prompt
        if is_new_session and skill_context:
            prompt += f"\n\n{skill_context}"
        if skill_name and skill_body:
            prompt += f"\n\n## Active Skill: {skill_name}\n{skill_body}"
        return prompt

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

        # Detect /skill-name prefix and load skill content
        skill_name, user_message, skill_body = self._extract_skill(user_message)
        if skill_name and not skill_body:
            yield ChatMessage(
                sender="system",
                text=f"Skill '{skill_name}' not found. Check installed plugins.",
                is_system=True,
            )
            return

        # Build skill context once per message (descriptions list for new sessions)
        skill_context = self._build_skill_context()

        room_key = (workspace.name, room.name)

        leader_is_new = self._storage.get_session_id(workspace, room.name, leader.name) is None
        leader_prompt = self._build_leader_prompt(
            leader, room, skill_context, skill_name, skill_body, is_new_session=leader_is_new
        )

        self._clear_live_steps(room_key)
        agent_name_for_cb = leader.name
        yield ChatMessage(sender=room.leader, text="", is_loading=True)
        try:
            response = await self._runner.query(
                leader,
                workspace,
                room.name,
                user_message,
                output_format={"type": "json_schema", "schema": ROUTING_SCHEMA},
                system_prompt_override=leader_prompt,
                step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
            )
            self._update_tokens_from_result(room_key, leader.name, response.usage)
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
                agent_is_new = self._storage.get_session_id(workspace, room.name, target_name) is None
                agent_system = self._build_agent_prompt(agent, skill_context, skill_name, skill_body, is_new_session=agent_is_new)

                self._clear_live_steps(room_key)
                agent_name_for_cb = target_name
                yield ChatMessage(sender=target_name, text="", is_loading=True)
                if agent.streaming:
                    text_parts: list[str] = []
                    streaming_steps: list[dict] = []
                    async for chunk in self._runner.query_streaming(
                        agent, workspace, room.name, forward_prompt,
                        system_prompt_override=agent_system,
                        step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                    ):
                        if chunk.done:
                            streaming_steps = chunk.steps
                            self._update_tokens_from_result(room_key, target_name, chunk.usage)
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
                            agent, workspace, room.name, forward_prompt,
                            system_prompt_override=agent_system,
                            step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                        )
                        self._update_tokens_from_result(room_key, target_name, agent_resp.usage)
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

                self._clear_live_steps(room_key)
                agent_name_for_cb = leader.name
                yield ChatMessage(sender=room.leader, text="", is_loading=True)
                try:
                    synthesis = await self._runner.query(
                        leader,
                        workspace,
                        room.name,
                        synthesis_prompt,
                        step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                    )
                    self._update_tokens_from_result(room_key, leader.name, synthesis.usage)
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
                chain_is_new = self._storage.get_session_id(workspace, room.name, agent_name) is None
                chain_agent_system = self._build_agent_prompt(agent, skill_context, skill_name, skill_body, is_new_session=chain_is_new)

                self._clear_live_steps(room_key)
                agent_name_for_cb = agent_name
                yield ChatMessage(sender=agent_name, text="", is_loading=True)
                if agent.streaming:
                    text_parts_chain: list[str] = []
                    chain_steps: list[dict] = []
                    async for chunk in self._runner.query_streaming(
                        agent, workspace, room.name, chain_prompt,
                        system_prompt_override=chain_agent_system,
                        step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                    ):
                        if chunk.done:
                            chain_steps = chunk.steps
                            self._update_tokens_from_result(room_key, agent_name, chunk.usage)
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
                            agent, workspace, room.name, chain_prompt,
                            system_prompt_override=chain_agent_system,
                            step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                        )
                        self._update_tokens_from_result(room_key, agent_name, agent_resp.usage)
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

                self._clear_live_steps(room_key)
                agent_name_for_cb = leader.name
                yield ChatMessage(sender=room.leader, text="", is_loading=True)
                try:
                    synthesis = await self._runner.query(
                        leader,
                        workspace,
                        room.name,
                        synthesis_prompt,
                        step_callback=lambda step, _name=agent_name_for_cb: self._add_live_step(room_key, step, _name),
                    )
                    self._update_tokens_from_result(room_key, leader.name, synthesis.usage)
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
