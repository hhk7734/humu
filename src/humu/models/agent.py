from pydantic import BaseModel, Field

from humu.config import DEFAULT_MODEL, DEFAULT_PROVIDER


class AgentConfig(BaseModel):
    name: str
    description: str
    system_prompt: str
    provider: str = DEFAULT_PROVIDER
    model: str = DEFAULT_MODEL
    mcp_servers: list[str] = Field(default_factory=list)
    streaming: bool = False
