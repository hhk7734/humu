from pydantic import BaseModel, computed_field


class Workspace(BaseModel):
    name: str
    root_path: str

    @computed_field
    @property
    def slug(self) -> str:
        return self.name.replace(" ", "-").lower()
