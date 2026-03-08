import pytest
import pytest_asyncio
from httpx import AsyncClient, ASGITransport

from humu.server.app import create_app


@pytest_asyncio.fixture
async def app(tmp_path):
    app = create_app(db_path=tmp_path / "test.db")
    async with app.router.lifespan_context(app):
        yield app


@pytest.mark.asyncio
async def test_full_flow(app):
    """Create workspace, room, agent via REST, then verify data."""
    transport = ASGITransport(app=app)

    async with AsyncClient(transport=transport, base_url="http://test") as client:
        # Create workspace
        resp = await client.post(
            "/api/workspaces",
            json={"name": "test", "root_path": "/tmp"},
        )
        assert resp.status_code == 201
        data = resp.json()
        assert data["name"] == "test"
        assert data["root_path"] == "/tmp"
        assert data["slug"] == "test"

        # Create agent (leader)
        resp = await client.post(
            "/api/workspaces/test/agents",
            json={
                "name": "leader",
                "description": "Room leader",
                "system_prompt": "You are a leader.",
            },
        )
        assert resp.status_code == 201
        data = resp.json()
        assert data["name"] == "leader"
        assert data["description"] == "Room leader"
        assert data["system_prompt"] == "You are a leader."

        # Create room
        resp = await client.post(
            "/api/workspaces/test/rooms",
            json={"name": "dev", "leader": "leader"},
        )
        assert resp.status_code == 201
        data = resp.json()
        assert data["name"] == "dev"
        assert data["leader"] == "leader"

        # Verify all data persists
        resp = await client.get("/api/workspaces")
        assert resp.status_code == 200
        workspaces = resp.json()
        assert len(workspaces) == 1
        assert workspaces[0]["name"] == "test"

        resp = await client.get("/api/workspaces/test/agents")
        assert resp.status_code == 200
        agents = resp.json()
        assert len(agents) == 1
        assert agents[0]["name"] == "leader"

        resp = await client.get("/api/workspaces/test/rooms")
        assert resp.status_code == 200
        rooms = resp.json()
        assert len(rooms) == 1
        assert rooms[0]["name"] == "dev"
        assert rooms[0]["leader"] == "leader"
