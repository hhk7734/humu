from __future__ import annotations

from pathlib import Path

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.events import Key
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, OptionList
from textual.widgets.option_list import Option

from humu.models.workspace import Workspace

MAX_SUGGESTIONS = 10


def _list_dirs(value: str) -> list[str]:
    if not value:
        return []

    p = Path(value).expanduser()

    if value.endswith("/") and p.is_dir():
        parent = p
        prefix = ""
    else:
        parent = p.parent
        prefix = p.name

    if not parent.is_dir():
        return []

    try:
        matches = sorted(
            str(c) + "/"
            for c in parent.iterdir()
            if c.is_dir()
            and not c.name.startswith(".")
            and c.name.startswith(prefix)
        )
    except PermissionError:
        return []

    return matches[:MAX_SUGGESTIONS]


class CreateWorkspaceScreen(ModalScreen[Workspace | None]):
    BINDINGS = [("escape", "cancel", "Cancel")]

    DEFAULT_CSS = """
    CreateWorkspaceScreen {
        align: center middle;
    }
    CreateWorkspaceScreen > Vertical {
        width: 70;
        height: auto;
        max-height: 80%;
        border: thick $accent;
        padding: 1 2;
        background: $surface;
    }
    CreateWorkspaceScreen Label {
        margin: 1 0 0 0;
    }
    CreateWorkspaceScreen Input {
        margin: 0 0 0 0;
    }
    CreateWorkspaceScreen Button {
        margin: 1 1 0 0;
    }
    CreateWorkspaceScreen #path-options {
        height: auto;
        max-height: 10;
        display: none;
        margin: 0 0 1 0;
    }
    CreateWorkspaceScreen #path-options.visible {
        display: block;
    }
    CreateWorkspaceScreen #path-options.visible:focus-within {
        border: solid $accent;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self._dropdown_visible = False
        self._dirs: list[str] = []

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Workspace", id="title")
            yield Label("Name:")
            yield Input(placeholder="my-project", id="ws-name")
            yield Label("Root path:")
            yield Input(placeholder="~/projects/my-app", id="ws-path")
            yield OptionList(id="path-options")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def _update_suggestions(self, value: str) -> None:
        options = self.query_one("#path-options", OptionList)
        self._dirs = _list_dirs(value)
        options.clear_options()
        if self._dirs:
            for d in self._dirs:
                options.add_option(Option(d))
            options.add_class("visible")
            self._dropdown_visible = True
        else:
            options.remove_class("visible")
            self._dropdown_visible = False

    def _accept_selection(self, value: str) -> None:
        path_input = self.query_one("#ws-path", Input)
        path_input.value = value
        path_input.cursor_position = len(value)
        path_input.focus()
        options = self.query_one("#path-options", OptionList)
        options.remove_class("visible")
        self._dropdown_visible = False

    def on_input_changed(self, event: Input.Changed) -> None:
        if event.input.id == "ws-path":
            self._update_suggestions(event.value)

    def on_key(self, event: Key) -> None:
        path_input = self.query_one("#ws-path", Input)

        # Only intercept when path input is focused and dropdown is visible
        if path_input != self.focused:
            return
        if not self._dropdown_visible:
            if event.key == "tab" and self._dirs:
                # Tab with no dropdown but dirs available: autocomplete first
                event.prevent_default()
                event.stop()
                self._accept_selection(self._dirs[0])
            return

        options = self.query_one("#path-options", OptionList)

        if event.key == "down":
            event.prevent_default()
            event.stop()
            options.action_cursor_down()
        elif event.key == "up":
            event.prevent_default()
            event.stop()
            options.action_cursor_up()
        elif event.key == "tab":
            event.prevent_default()
            event.stop()
            highlighted = options.highlighted
            if highlighted is not None:
                option = options.get_option_at_index(highlighted)
                self._accept_selection(str(option.prompt))
            elif self._dirs:
                self._accept_selection(self._dirs[0])

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        if event.option_list.id == "path-options":
            self._accept_selection(str(event.option.prompt))

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#ws-name", Input).value.strip()
            path = self.query_one("#ws-path", Input).value.strip()
            if name and path:
                self.dismiss(Workspace(name=name, root_path=str(Path(path).expanduser())))
            else:
                self.notify("Name and path are required.", severity="error")
        else:
            self.dismiss(None)

    def action_cancel(self) -> None:
        self.dismiss(None)
