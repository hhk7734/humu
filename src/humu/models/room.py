from pydantic import BaseModel, Field


class Room(BaseModel):
    name: str
    leader: str
    agents: list[str] = Field(default_factory=list)
