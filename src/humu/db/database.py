import logging

import aiosqlite

logger = logging.getLogger(__name__)

SCHEMA_VERSION = 2


class Database:
    def __init__(self, db_path: str | object) -> None:
        self._db_path = str(db_path)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(self._db_path)
        self._conn.row_factory = aiosqlite.Row

        await self._conn.executescript("""
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );
        """)

        cursor = await self._conn.execute("SELECT version FROM schema_version")
        row = await cursor.fetchone()
        current_version = row[0] if row else 0

        if current_version < SCHEMA_VERSION:
            await self._migrate(current_version)

        await self._conn.execute("PRAGMA foreign_keys = ON")
        await self._conn.commit()

    async def _migrate(self, from_version: int) -> None:
        logger.info("Migrating database from version %d to %d", from_version, SCHEMA_VERSION)

        # Drop old tables that have incompatible schemas
        if from_version < 2:
            await self._conn.executescript("""
                DROP TABLE IF EXISTS agents;
                DROP TABLE IF EXISTS messages;
                DROP TABLE IF EXISTS rooms;
                DROP TABLE IF EXISTS workspaces;
            """)

        await self._conn.executescript("""
            CREATE TABLE IF NOT EXISTS workspaces (
                name TEXT PRIMARY KEY,
                root_path TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS rooms (
                workspace TEXT NOT NULL,
                name TEXT NOT NULL,
                leader TEXT NOT NULL,
                agents TEXT NOT NULL DEFAULT '[]',
                PRIMARY KEY (workspace, name),
                FOREIGN KEY (workspace) REFERENCES workspaces(name) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS agents (
                workspace TEXT NOT NULL,
                room TEXT NOT NULL,
                name TEXT NOT NULL,
                config TEXT NOT NULL,
                PRIMARY KEY (workspace, room, name),
                FOREIGN KEY (workspace, room) REFERENCES rooms(workspace, name) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace TEXT NOT NULL,
                room TEXT NOT NULL,
                data TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
        """)

        # Update version
        await self._conn.execute("DELETE FROM schema_version")
        await self._conn.execute(
            "INSERT INTO schema_version (version) VALUES (?)", (SCHEMA_VERSION,)
        )
        await self._conn.commit()
        logger.info("Database migration complete")

    @property
    def conn(self) -> aiosqlite.Connection:
        assert self._conn is not None, "Database not initialized"
        return self._conn

    async def close(self) -> None:
        if self._conn:
            await self._conn.close()
            self._conn = None
