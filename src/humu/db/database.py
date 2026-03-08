import aiosqlite


class Database:
    def __init__(self, db_path: str | object) -> None:
        self._db_path = str(db_path)
        self._conn: aiosqlite.Connection | None = None

    async def initialize(self) -> None:
        self._conn = await aiosqlite.connect(self._db_path)
        self._conn.row_factory = aiosqlite.Row
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
        await self._conn.execute("PRAGMA foreign_keys = ON")
        await self._conn.commit()

    @property
    def conn(self) -> aiosqlite.Connection:
        assert self._conn is not None, "Database not initialized"
        return self._conn

    async def close(self) -> None:
        if self._conn:
            await self._conn.close()
            self._conn = None
