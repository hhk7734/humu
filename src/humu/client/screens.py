from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Select, Static, Switch, TextArea


class CreateWorkspaceScreen(ModalScreen[dict | None]):
    DEFAULT_CSS = """
    CreateWorkspaceScreen {
        align: center middle;
    }
    CreateWorkspaceScreen > Vertical {
        width: 60;
        height: auto;
        max-height: 20;
        border: thick $accent;
        padding: 1 2;
    }
    """

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Workspace", classes="panel-title")
            yield Label("Name")
            yield Input(id="ws-name", placeholder="workspace name")
            yield Label("Root Path")
            yield Input(id="ws-root-path", placeholder="/path/to/project")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#ws-name", Input).value.strip()
            root_path = self.query_one("#ws-root-path", Input).value.strip()
            if name and root_path:
                self.dismiss({"name": name, "root_path": root_path})
            else:
                self.notify("Name and root path are required", severity="error")
        elif event.button.id == "btn-cancel":
            self.dismiss(None)

    def on_key(self, event) -> None:
        if event.key == "escape":
            self.dismiss(None)


class CreateRoomScreen(ModalScreen[str | None]):
    DEFAULT_CSS = """
    CreateRoomScreen {
        align: center middle;
    }
    CreateRoomScreen > Vertical {
        width: 60;
        height: auto;
        max-height: 14;
        border: thick $accent;
        padding: 1 2;
    }
    """

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Room", classes="panel-title")
            yield Label("Room Name")
            yield Input(id="room-name", placeholder="room name")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#room-name", Input).value.strip()
            if name:
                self.dismiss(name)
            else:
                self.notify("Room name is required", severity="error")
        elif event.button.id == "btn-cancel":
            self.dismiss(None)

    def on_key(self, event) -> None:
        if event.key == "escape":
            self.dismiss(None)


class CreateAgentScreen(ModalScreen[dict | None]):
    """Used for both create and edit. For edit, pass agent_data to pre-fill fields."""

    DEFAULT_CSS = """
    CreateAgentScreen {
        align: center middle;
    }
    CreateAgentScreen > Vertical {
        width: 70;
        height: auto;
        max-height: 30;
        border: thick $accent;
        padding: 1 2;
        overflow-y: auto;
    }
    CreateAgentScreen TextArea {
        height: 5;
    }
    """

    def __init__(self, agent_data: dict | None = None) -> None:
        super().__init__()
        self._agent_data = agent_data
        self._is_edit = agent_data is not None

    def compose(self) -> ComposeResult:
        d = self._agent_data or {}
        title = "Edit Agent" if self._is_edit else "Create Agent"
        btn_label = "Save" if self._is_edit else "Create"

        with Vertical():
            yield Label(title, classes="panel-title")

            yield Label("Name")
            yield Input(
                id="agent-name",
                value=d.get("name", ""),
                disabled=self._is_edit,
                placeholder="agent name",
            )

            yield Label("Description")
            yield Input(
                id="agent-desc",
                value=d.get("description", ""),
                placeholder="what does this agent do?",
            )

            yield Label("System Prompt")
            yield TextArea(
                d.get("system_prompt", ""),
                id="agent-prompt",
            )

            yield Label("Provider")
            yield Select(
                [("anthropic", "anthropic"), ("openai", "openai")],
                value=d.get("provider", "anthropic"),
                id="agent-provider",
            )

            yield Label("Model")
            yield Input(
                id="agent-model",
                value=d.get("model", "claude-opus-4-6"),
                placeholder="model identifier",
            )

            yield Static("Streaming")
            yield Switch(value=d.get("streaming", False), id="agent-streaming")

            yield Button(btn_label, variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#agent-name", Input).value.strip()
            desc = self.query_one("#agent-desc", Input).value.strip()
            prompt = self.query_one("#agent-prompt", TextArea).text.strip()
            provider = self.query_one("#agent-provider", Select).value
            model = self.query_one("#agent-model", Input).value.strip()
            streaming = self.query_one("#agent-streaming", Switch).value

            if not name or not desc or not prompt:
                self.notify(
                    "Name, description, and system prompt are required",
                    severity="error",
                )
                return

            self.dismiss(
                {
                    "name": name,
                    "description": desc,
                    "system_prompt": prompt,
                    "provider": provider,
                    "model": model,
                    "streaming": streaming,
                }
            )
        elif event.button.id == "btn-cancel":
            self.dismiss(None)

    def on_key(self, event) -> None:
        if event.key == "escape":
            self.dismiss(None)


class ConfirmDeleteScreen(ModalScreen[bool]):
    DEFAULT_CSS = """
    ConfirmDeleteScreen {
        align: center middle;
    }
    ConfirmDeleteScreen > Vertical {
        width: 60;
        height: auto;
        max-height: 12;
        border: thick $error;
        padding: 1 2;
    }
    """

    def __init__(self, message: str) -> None:
        super().__init__()
        self._message = message

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Confirm Delete", classes="panel-title")
            yield Static(self._message)
            yield Button("Delete", variant="error", id="btn-delete")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-delete":
            self.dismiss(True)
        elif event.button.id == "btn-cancel":
            self.dismiss(False)

    def on_key(self, event) -> None:
        if event.key == "escape":
            self.dismiss(False)
