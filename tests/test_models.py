from humu.models.workspace import Workspace
from humu.models.room import Room
from humu.models.agent import AgentConfig
from humu.models.message import ChatMessage


def test_workspace_creation():
    ws = Workspace(name="my-app", root_path="/home/user/my-app")
    assert ws.name == "my-app"
    assert ws.slug == "my-app"


def test_workspace_slug_spaces():
    ws = Workspace(name="My App", root_path="/tmp")
    assert ws.slug == "my-app"


def test_room_creation():
    room = Room(name="implement", leader="leader-agent")
    assert room.name == "implement"
    assert room.agents == []


def test_agent_config_defaults():
    agent = AgentConfig(
        name="coder",
        description="Writes code",
        system_prompt="You are a coder.",
    )
    assert agent.provider == "anthropic"
    assert agent.mcp_servers == []


def test_chat_message():
    msg = ChatMessage(
        workspace="my-app",
        room="implement",
        sender="leader",
        text="Hello",
    )
    assert msg.is_system is False
