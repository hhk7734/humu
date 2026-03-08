from pathlib import Path

HUMU_HOME = Path.home() / ".humu"
HUMU_DB = HUMU_HOME / "humu.db"
PLUGINS_DIR = HUMU_HOME / "plugins"
MARKETPLACES_DIR = HUMU_HOME / "marketplaces"

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 9130

DEFAULT_PROVIDER = "anthropic"
DEFAULT_MODEL = "claude-opus-4-6"
