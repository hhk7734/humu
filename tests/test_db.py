import pytest
import pytest_asyncio

from humu.db.database import Database
from humu.db.repositories import Repository
from humu.models.workspace import Workspace
from humu.models.room import Room
from humu.models.agent import AgentConfig


@pytest_asyncio.fixture
async def db(tmp_path):
    database = Database(tmp_path / "test.db")
    await database.initialize()
    yield database
    await database.close()


@pytest_asyncio.fixture
async def repo(db):
    return Repository(db)


@pytest.mark.asyncio
async def test_workspace_crud(repo):
    ws = Workspace(name="test", root_path="/tmp/test")
    await repo.save_workspace(ws)
    result = await repo.get_workspace("test")
    assert result is not None
    assert result.root_path == "/tmp/test"

    all_ws = await repo.list_workspaces()
    assert len(all_ws) == 1

    await repo.delete_workspace("test")
    assert await repo.get_workspace("test") is None


@pytest.mark.asyncio
async def test_room_crud(repo):
    ws = Workspace(name="test", root_path="/tmp/test")
    await repo.save_workspace(ws)

    room = Room(name="dev", leader="leader")
    await repo.save_room("test", room)
    result = await repo.get_room("test", "dev")
    assert result is not None
    assert result.leader == "leader"

    rooms = await repo.list_rooms("test")
    assert len(rooms) == 1


@pytest.mark.asyncio
async def test_agent_crud(repo):
    ws = Workspace(name="test", root_path="/tmp/test")
    await repo.save_workspace(ws)

    room = Room(name="dev", leader="leader")
    await repo.save_room("test", room)

    agent = AgentConfig(
        name="coder",
        description="Writes code",
        system_prompt="You are a coder.",
    )
    await repo.save_agent("test", "dev", agent)
    result = await repo.get_agent("test", "dev", "coder")
    assert result is not None
    assert result.description == "Writes code"

    agents = await repo.list_agents("test", "dev")
    assert len(agents) == 1
    assert agents[0].name == "coder"

    await repo.delete_agent("test", "dev", "coder")
    assert await repo.get_agent("test", "dev", "coder") is None


@pytest.mark.asyncio
async def test_create_room_auto_leader(repo):
    ws = Workspace(name="test", root_path="/tmp/test")
    await repo.save_workspace(ws)

    room = await repo.create_room_with_leader("test", "dev")
    assert room.name == "dev"
    assert room.leader == "leader"

    saved_room = await repo.get_room("test", "dev")
    assert saved_room is not None
    assert saved_room.leader == "leader"

    leader = await repo.get_agent("test", "dev", "leader")
    assert leader is not None
    assert leader.name == "leader"
    assert leader.system_prompt != ""


@pytest.mark.asyncio
async def test_chat_history(repo):
    ws = Workspace(name="test", root_path="/tmp/test")
    await repo.save_workspace(ws)

    await repo.append_message("test", "dev", {
        "sender": "user", "text": "hello"
    })
    history = await repo.get_messages("test", "dev")
    assert len(history) == 1
    assert history[0]["sender"] == "user"
