from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, Input, Label, Select, TextArea

from humu.config import DEFAULT_MODEL, DEFAULT_TOOLS
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

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Agent", id="title")
            yield Label("Name:")
            yield Input(placeholder="backend-expert", id="agent-name")
            yield Label("Description:")
            yield Input(
                placeholder="Expert in backend development...",
                id="agent-desc",
            )
            yield Label("System prompt:")
            yield TextArea(id="agent-prompt")
            yield Label("Model:")
            yield Select(
                self.MODELS,
                value=DEFAULT_MODEL,
                id="model-select",
            )
            yield Label("Tools (comma-separated):")
            yield Input(
                value=", ".join(DEFAULT_TOOLS),
                id="agent-tools",
            )
            yield Checkbox("Enable streaming", id="agent-streaming")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#agent-name", Input).value.strip()
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
