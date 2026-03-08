from __future__ import annotations

from typing import Any, Annotated

from pydantic import BaseModel, Field
import operator


class AgentTask(BaseModel):
    agent_name: str
    context: str
    depends_on: list[str] = Field(default_factory=list)


class RoomState(BaseModel):
    workspace: str
    room: str
    user_message: str
    leader_config: dict
    agent_configs: dict[str, dict]

    # Set by leader planning
    agent_tasks: list[AgentTask] = Field(default_factory=list)

    # Accumulated by agent executions
    agent_results: Annotated[dict[str, str], operator.or_] = Field(default_factory=dict)

    # Set by leader aggregation
    final_response: str | None = None

    # Event log for broadcasting
    events: Annotated[list[dict], operator.add] = Field(default_factory=list)
