from pathlib import Path

HUMU_HOME = Path.home() / ".humu"
WORKSPACES_FILE = HUMU_HOME / "workspaces.json"
PROJECTS_DIR = HUMU_HOME / "projects"
MARKETPLACES_FILE = HUMU_HOME / "marketplaces.json"
PLUGINS_DIR = HUMU_HOME / "plugins"
SKILLS_CONFIG_FILE = HUMU_HOME / "skills_config.json"

DEFAULT_TOOLS = ["Read", "Grep", "Glob"]
DEFAULT_MODEL = "opus"

# Model context window sizes (in tokens)
MODEL_CONTEXT_WINDOWS: dict[str, int] = {
    "sonnet": 200_000,
    "claude-sonnet-4-20250514": 200_000,
    "opus": 200_000,
    "claude-opus-4-20250514": 200_000,
    "haiku": 200_000,
    "claude-haiku-4-20250506": 200_000,
    "claude-3-5-sonnet-20241022": 200_000,
    "claude-3-5-haiku-20241022": 200_000,
}
DEFAULT_CONTEXT_WINDOW = 200_000

ROUTING_SCHEMA = {
    "type": "object",
    "properties": {
        "action": {
            "type": "string",
            "enum": ["direct", "forward", "chain"],
        },
        "message": {
            "type": "string",
            "description": "Response text when action is 'direct'",
        },
        "targets": {
            "type": "array",
            "items": {"type": "string"},
            "description": "Agent names to forward to when action is 'forward'",
        },
        "context": {
            "type": "string",
            "description": "Context to include when forwarding",
        },
        "steps": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "agent": {"type": "string"},
                    "context": {"type": "string"},
                },
                "required": ["agent", "context"],
            },
            "description": "Sequential agent steps when action is 'chain'",
        },
    },
    "required": ["action"],
}
