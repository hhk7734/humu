from __future__ import annotations

import logging
from contextlib import asynccontextmanager
from pathlib import Path

import uvicorn
from fastapi import FastAPI

from humu.config import DEFAULT_HOST, DEFAULT_PORT, HUMU_DB, HUMU_HOME
from humu.db.database import Database
from humu.db.repositories import Repository
from humu.engine.room_graph import RoomEngine
from humu.providers.registry import ProviderRegistry
from humu.server.routes import create_router
from humu.server.ws import WebSocketManager

logger = logging.getLogger(__name__)


def create_app(db_path: str | Path | None = None) -> FastAPI:
    db = Database(db_path or HUMU_DB)
    repo = Repository(db)
    providers = ProviderRegistry()
    engine = RoomEngine(providers)
    ws_manager = WebSocketManager()

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        HUMU_HOME.mkdir(parents=True, exist_ok=True)
        await db.initialize()
        yield
        await db.close()

    app = FastAPI(lifespan=lifespan)
    app.state.db = db
    app.state.repo = repo
    app.state.providers = providers
    app.state.engine = engine
    app.state.ws_manager = ws_manager

    app.include_router(create_router())

    return app


def run_server() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    HUMU_HOME.mkdir(parents=True, exist_ok=True)
    app = create_app()
    uvicorn.run(app, host=DEFAULT_HOST, port=DEFAULT_PORT, log_level="info")
