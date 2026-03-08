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

        # Create room (auto-creates leader)
        resp = await client.post(
            "/api/workspaces/test/rooms",
            json={"name": "dev"},
        )
        assert resp.status_code == 201
        data = resp.json()
        assert data["name"] == "dev"
        assert data["leader"] == "leader"

        # Verify auto-created leader agent
        resp = await client.get("/api/workspaces/test/rooms/dev/agents")
        assert resp.status_code == 200
        agents = resp.json()
        assert len(agents) == 1
        assert agents[0]["name"] == "leader"

        # Add another agent to room
        resp = await client.post(
            "/api/workspaces/test/rooms/dev/agents",
            json={
                "name": "coder",
                "description": "Code writer",
                "system_prompt": "You write code.",
            },
        )
        assert resp.status_code == 201
        data = resp.json()
        assert data["name"] == "coder"

        # Verify agent count
        resp = await client.get("/api/workspaces/test/rooms/dev/agents")
        assert resp.status_code == 200
        agents = resp.json()
        assert len(agents) == 2

        # Verify all data persists
        resp = await client.get("/api/workspaces")
        assert resp.status_code == 200
        workspaces = resp.json()
        assert len(workspaces) == 1
        assert workspaces[0]["name"] == "test"

        resp = await client.get("/api/workspaces/test/rooms")
        assert resp.status_code == 200
        rooms = resp.json()
        assert len(rooms) == 1
        assert rooms[0]["name"] == "dev"
        assert rooms[0]["leader"] == "leader"
