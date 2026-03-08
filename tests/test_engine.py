import pytest
from unittest.mock import AsyncMock, MagicMock

from humu.engine.state import RoomState, AgentTask
from humu.engine.room_graph import RoomEngine
from humu.providers.base import LLMResponse, Message
from humu.models.agent import AgentConfig
from humu.models.room import Room


def test_room_state_initial():
    state = RoomState(
        workspace="ws",
        room="dev",
        user_message="hello",
        leader_config={},
        agent_configs={},
    )
    assert state.agent_tasks == []
    assert state.agent_results == {}
    assert state.final_response is None


def test_agent_task_creation():
    task = AgentTask(agent_name="coder", context="write code", depends_on=[])
    assert task.agent_name == "coder"
    assert task.depends_on == []
