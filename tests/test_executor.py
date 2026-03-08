import pytest
from unittest.mock import AsyncMock

from humu.server.executor import RoomExecutor
from humu.models.room import Room
from humu.models.agent import AgentConfig
from humu.providers.base import LLMResponse, Message


@pytest.mark.asyncio
async def test_executor_direct_response(tmp_path):
    """Leader responds directly without delegating."""
    from humu.db.database import Database
    from humu.db.repositories import Repository
    from humu.providers.registry import ProviderRegistry
    from humu.engine.room_graph import RoomEngine
    from humu.server.ws import WebSocketManager

    db = Database(tmp_path / "test.db")
    await db.initialize()
    repo = Repository(db)
    providers = ProviderRegistry()
    engine = RoomEngine(providers)
    ws_manager = WebSocketManager()

    # Setup workspace, room, agent
    from humu.models.workspace import Workspace

    await repo.save_workspace(Workspace(name="test", root_path="/tmp"))
    await repo.save_room("test", Room(name="dev", leader="leader"))
    await repo.save_agent(
        "test",
        AgentConfig(
            name="leader",
            description="Room leader",
            system_prompt="You are a leader.",
        ),
    )

    executor = RoomExecutor(repo, engine, ws_manager)

    # Mock the provider to return a direct response
    mock_provider = AsyncMock()
    mock_provider.chat.return_value = LLMResponse(
        text='{"action": "direct", "message": "Hello!"}',
    )
    providers._providers["anthropic"] = mock_provider

    events = []
    async for event in executor.execute("test", "dev", "hello"):
        events.append(event)

    assert any(e.get("type") == "message_added" for e in events)
    await db.close()
