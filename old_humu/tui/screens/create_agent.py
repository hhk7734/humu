from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Input, Label, Select, TextArea

from humu.config import (
    DEFAULT_MODEL,
    DEFAULT_TOOLS,
)
from humu.models.agent import AgentConfig


class CreateAgentScreen(ModalScreen[AgentConfig | None]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    DEFAULT_CSS = """
    CreateAgentScreen {
        align: center middle;
    }
    CreateAgentScreen > Vertical {
        width: 70;
        height: auto;
        max-height: 80%;
        border: thick $accent;
        padding: 1 2;
        background: $surface;
        overflow-y: auto;
    }
    CreateAgentScreen Label {
        margin: 1 0 0 0;
    }
    CreateAgentScreen Input {
        margin: 0 0 1 0;
    }
    CreateAgentScreen TextArea {
        height: 6;
        margin: 0 0 1 0;
    }
    CreateAgentScreen Select {
        margin: 0 0 1 0;
    }
    CreateAgentScreen Button {
        margin: 1 1 0 0;
    }
    """

    MODELS = [("opus", "opus"), ("sonnet", "sonnet"), ("haiku", "haiku")]

    def __init__(self, existing: AgentConfig | None = None) -> None:
        super().__init__()
        self._existing = existing
        self._is_edit = existing is not None

    def compose(self) -> ComposeResult:
        ex = self._existing
        title = "Edit Agent" if self._is_edit else "Create Agent"
        btn_label = "Save" if self._is_edit else "Create"
        with Vertical():
            yield Label(title, id="title")
            yield Label("Name:")
            yield Input(
                value=ex.name if ex else "",
                placeholder="backend-expert",
                id="agent-name",
                disabled=self._is_edit,  # name is the key, don't allow rename
            )
            yield Label("Description:")
            yield Input(
                value=ex.description if ex else "",
                placeholder="Expert in backend development...",
                id="agent-desc",
            )
            yield Label("System prompt:")
            yield TextArea(ex.prompt if ex else "", id="agent-prompt")
            yield Label("Model:")
            yield Select(
                self.MODELS,
                value=ex.model if ex else DEFAULT_MODEL,
                id="model-select",
            )
            yield Label("Tools (comma-separated):")
            yield Input(
                value=", ".join(ex.tools) if ex else ", ".join(DEFAULT_TOOLS),
                id="agent-tools",
            )
            yield Checkbox(
                "Enable streaming",
                value=ex.streaming if ex else False,
                id="agent-streaming",
            )
            yield Button(btn_label, variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#agent-name", Input).value.strip()
            if not name and self._existing:
                name = self._existing.name
            desc = self.query_one("#agent-desc", Input).value.strip()
            prompt = self.query_one("#agent-prompt", TextArea).text.strip()
            model = str(self.query_one("#model-select", Select).value)
            tools_raw = self.query_one("#agent-tools", Input).value.strip()
            streaming = self.query_one("#agent-streaming", Checkbox).value

            if not name or not desc or not prompt:
                self.notify(
                    "Name, description, and prompt are required.",
                    severity="error",
                )
                return

            tools = [t.strip() for t in tools_raw.split(",") if t.strip()]

            self.dismiss(
                AgentConfig(
                    name=name,
                    description=desc,
                    prompt=prompt,
                    model=model,
                    tools=tools,
                    streaming=streaming,
                )
            )
        else:
            self.dismiss(None)

    def action_cancel(self) -> None:
        self.dismiss(None)
