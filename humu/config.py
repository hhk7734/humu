from pathlib import Path

HUMU_HOME = Path.home() / ".humu"
AGENTS_DIR = HUMU_HOME / "agents"
WORKSPACES_FILE = HUMU_HOME / "workspaces.json"
PROJECTS_DIR = HUMU_HOME / "projects"
MARKETPLACES_FILE = HUMU_HOME / "marketplaces.json"
PLUGINS_DIR = HUMU_HOME / "plugins"
SKILLS_CONFIG_FILE = HUMU_HOME / "skills_config.json"

DEFAULT_TOOLS = ["Read", "Grep", "Glob"]
DEFAULT_MODEL = "sonnet"

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
