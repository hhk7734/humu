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
async def test_health_check(app):
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/health")
        assert resp.status_code == 200
        assert resp.json() == {"status": "ok"}


@pytest.mark.asyncio
async def test_list_workspaces_empty(app):
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.get("/api/workspaces")
        assert resp.status_code == 200
        assert resp.json() == []


@pytest.mark.asyncio
async def test_create_workspace(app):
    transport = ASGITransport(app=app)
    async with AsyncClient(transport=transport, base_url="http://test") as client:
        resp = await client.post(
            "/api/workspaces",
            json={"name": "test", "root_path": "/tmp/test"},
        )
        assert resp.status_code == 201

        resp = await client.get("/api/workspaces")
        assert len(resp.json()) == 1
