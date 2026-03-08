from __future__ import annotations

import json
import logging
from typing import Any

from langgraph.graph import StateGraph, START, END

from humu.engine.state import AgentTask, RoomState
from humu.providers.base import LLMProvider, Message
from humu.providers.registry import ProviderRegistry

logger = logging.getLogger(__name__)


class RoomEngine:
    def __init__(self, provider_registry: ProviderRegistry) -> None:
        self._providers = provider_registry

    async def _leader_plan(self, state: RoomState) -> dict:
        """Leader decides how to handle the user message."""
        leader = state.leader_config
        provider = self._providers.get(leader.get("provider", "anthropic"))

        agent_descriptions = []
        for name, config in state.agent_configs.items():
            agent_descriptions.append(f"- {name}: {config.get('description', '')}")

        agents_text = "\n".join(agent_descriptions) if agent_descriptions else "None"

        system_prompt = f"""{leader.get('system_prompt', '')}

You are the leader of room "{state.room}". Decide how to handle the user's message.

Available agents:
{agents_text}

Respond with JSON:
- Direct answer: {{"action": "direct", "message": "your response"}}
- Delegate to agents: {{"action": "delegate", "tasks": [{{"agent": "name", "context": "what to do", "depends_on": []}}]}}

Use "depends_on" to specify agent names whose output this task needs. Empty means run immediately (parallel with other independent tasks)."""

        response = await provider.chat(
            [Message(role="user", content=state.user_message)],
            model=leader.get("model", "claude-sonnet-4-20250514"),
            system_prompt=system_prompt,
        )

        try:
            decision = json.loads(response.text)
        except (json.JSONDecodeError, TypeError):
            return {
                "final_response": response.text,
                "events": [{"type": "leader_response", "text": response.text}],
            }

        action = decision.get("action", "direct")

        if action == "direct":
            return {
                "final_response": decision.get("message", response.text),
                "events": [{"type": "leader_response", "text": decision.get("message", response.text)}],
            }

        tasks = [
            AgentTask(
                agent_name=t["agent"],
                context=t.get("context", ""),
                depends_on=t.get("depends_on", []),
            )
            for t in decision.get("tasks", [])
        ]

        return {
            "agent_tasks": tasks,
            "events": [
                {
                    "type": "delegation",
                    "tasks": [t.model_dump() for t in tasks],
                }
            ],
        }

    async def _run_agent(self, state: RoomState, agent_name: str) -> dict:
        """Execute a single agent task."""
        config = state.agent_configs.get(agent_name, {})
        provider = self._providers.get(config.get("provider", "anthropic"))

        # Find the task for this agent
        task = next((t for t in state.agent_tasks if t.agent_name == agent_name), None)
        context = task.context if task else state.user_message

        # Include results from dependencies
        dep_context = ""
        if task and task.depends_on:
            dep_parts = []
            for dep in task.depends_on:
                if dep in state.agent_results:
                    dep_parts.append(f"[{dep}]: {state.agent_results[dep]}")
            if dep_parts:
                dep_context = "\n\nPrevious agent results:\n" + "\n\n".join(dep_parts)

        prompt = f"User message: {state.user_message}\n\nLeader's context: {context}{dep_context}"

        response = await provider.chat(
            [Message(role="user", content=prompt)],
            model=config.get("model", "claude-sonnet-4-20250514"),
            system_prompt=config.get("system_prompt", ""),
        )

        return {
            "agent_results": {agent_name: response.text},
            "events": [{"type": "agent_done", "agent": agent_name, "text": response.text}],
        }

    async def _leader_aggregate(self, state: RoomState) -> dict:
        """Leader synthesizes agent results."""
        if not state.agent_results:
            return {"final_response": "No agent results to synthesize."}

        leader = state.leader_config
        provider = self._providers.get(leader.get("provider", "anthropic"))

        results_text = "\n\n".join(
            f"[{name}]: {text}" for name, text in state.agent_results.items()
        )

        prompt = (
            f"User asked: {state.user_message}\n\n"
            f"Agent responses:\n\n{results_text}\n\n"
            f"Synthesize these into a coherent response."
        )

        response = await provider.chat(
            [Message(role="user", content=prompt)],
            model=leader.get("model", "claude-sonnet-4-20250514"),
            system_prompt=leader.get("system_prompt", ""),
        )

        return {
            "final_response": response.text,
            "events": [{"type": "leader_response", "text": response.text}],
        }

    def build_graph(self, room_state: RoomState) -> StateGraph:
        """Build a LangGraph StateGraph for this room interaction."""
        graph = StateGraph(RoomState)

        graph.add_node("leader_plan", self._leader_plan)
        graph.add_node("leader_aggregate", self._leader_aggregate)

        graph.add_edge(START, "leader_plan")

        # Add agent nodes for all known agents
        for agent_name in room_state.agent_configs:

            async def agent_fn(state: RoomState, name: str = agent_name) -> dict:
                return await self._run_agent(state, name)

            graph.add_node(agent_name, agent_fn)

        if not room_state.agent_tasks:
            # First pass: leader plans. Route based on result.
            def route_after_plan(state: RoomState) -> list[str] | str:
                if state.final_response is not None:
                    return END
                if state.agent_tasks:
                    first_wave = [
                        t.agent_name
                        for t in state.agent_tasks
                        if not t.depends_on
                    ]
                    return first_wave if first_wave else ["leader_aggregate"]
                return END

            graph.add_conditional_edges("leader_plan", route_after_plan)

        # Wire agent edges: agents with dependencies wait for them,
        # agents without dependencies go to leader_aggregate
        task_names = {t.agent_name for t in room_state.agent_tasks}
        depended_on_by: dict[str, list[str]] = {}
        for task in room_state.agent_tasks:
            for dep in task.depends_on:
                depended_on_by.setdefault(dep, []).append(task.agent_name)

        for agent_name in room_state.agent_configs:
            dependents = depended_on_by.get(agent_name, [])
            if dependents:
                # This agent's output feeds into dependent agents
                def route_after_agent(
                    state: RoomState, targets: list[str] = dependents
                ) -> list[str]:
                    return targets

                graph.add_conditional_edges(agent_name, route_after_agent)
            else:
                # No dependents — go straight to aggregation
                graph.add_edge(agent_name, "leader_aggregate")

        graph.add_edge("leader_aggregate", END)

        return graph
