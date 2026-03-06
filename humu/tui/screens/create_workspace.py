from __future__ import annotations

import os
from collections import deque
from pathlib import Path

from textual.app import ComposeResult
from textual.containers import Vertical
from textual.events import Key
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Static

from humu.models.workspace import Workspace

MAX_SUGGESTIONS = 10


def _dir_needle(text: str) -> str:
    """Normalise a path component for subsequence matching."""
    return text.lower().replace("-", "").replace("_", "")


def _is_subsequence(needle: str, haystack: str) -> bool:
    it = iter(haystack)
    return all(ch in it for ch in needle)


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

    seen: set[str] = set()
    results: list[str] = []

    # 1) Exact prefix matches first
    try:
        for c in sorted(parent.iterdir()):
            if len(results) >= MAX_SUGGESTIONS:
                break
            if c.is_dir() and not c.name.startswith(".") and c.name.startswith(prefix):
                key = str(c) + "/"
                if key not in seen:
                    seen.add(key)
                    results.append(key)
    except PermissionError:
        pass

    if len(results) >= MAX_SUGGESTIONS or not prefix:
        return results

    # 2) Fuzzy subsequence search (BFS up to 3 levels deep from parent)
    needle = _dir_needle(prefix)
    if needle:
        q: deque[tuple[Path, int]] = deque([(parent, 0)])
        while q and len(results) < MAX_SUGGESTIONS:
            cur, depth = q.popleft()
            try:
                entries = sorted(cur.iterdir(), key=lambda e: e.name)
            except OSError:
                continue
            for entry in entries:
                if len(results) >= MAX_SUGGESTIONS:
                    break
                if entry.name.startswith(".") or not entry.is_dir():
                    continue
                # Build the relative path from parent for haystack
                rel = entry.relative_to(parent)
                haystack = _dir_needle(str(rel).replace(os.sep, ""))
                if _is_subsequence(needle, haystack):
                    key = str(entry) + "/"
                    if key not in seen:
                        seen.add(key)
                        results.append(key)
                if depth < 3:
                    q.append((entry, depth + 1))

    return results


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
    CreateWorkspaceScreen #path-suggestions {
        height: 5;
        margin: 0 0 0 0;
        padding: 0 1;
        color: $text-muted;
        background: $surface-darken-1;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self._dirs: list[str] = []
        self._dir_index: int = 0

    def compose(self) -> ComposeResult:
        with Vertical():
            yield Label("Create Workspace", id="title")
            yield Label("Name:")
            yield Input(placeholder="my-project", id="ws-name")
            yield Label("Root path:")
            yield Input(placeholder="~/projects/my-app", id="ws-path")
            yield Static("", id="path-suggestions")
            yield Button("Create", variant="primary", id="btn-create")
            yield Button("Cancel", id="btn-cancel")

    def _update_suggestions(self, value: str) -> None:
        self._dirs = _list_dirs(value)
        self._dir_index = 0
        self._render_suggestions()

    def _render_suggestions(self) -> None:
        widget = self.query_one("#path-suggestions", Static)
        lines: list[str] = []
        if self._dirs:
            window = 5
            total = len(self._dirs)
            start = max(0, min(self._dir_index - window // 2, total - window))
            end = min(start + window, total)
            for i in range(start, end):
                d = self._dirs[i]
                if i == self._dir_index:
                    lines.append(f"[bold reverse] ❯ {d} [/bold reverse]")
                else:
                    lines.append(f"   {d}")
        # Always pad to 5 lines so the widget height never changes
        while len(lines) < 5:
            lines.append("")
        widget.update("\n".join(lines))

    def _accept_current(self) -> None:
        if not self._dirs:
            return
        selected = self._dirs[self._dir_index]
        path_input = self.query_one("#ws-path", Input)
        path_input.value = selected
        path_input.cursor_position = len(selected)
        path_input.focus()
        self._dirs = []
        self._dir_index = 0
        self._render_suggestions()

    def on_input_changed(self, event: Input.Changed) -> None:
        if event.input.id == "ws-path":
            self._update_suggestions(event.value)

    def on_key(self, event: Key) -> None:
        path_input = self.query_one("#ws-path", Input)
        if path_input != self.focused or not self._dirs:
            return

        if event.key == "down":
            self._dir_index = (self._dir_index + 1) % len(self._dirs)
            self._render_suggestions()
            event.prevent_default()
            event.stop()
        elif event.key == "up":
            self._dir_index = (self._dir_index - 1) % len(self._dirs)
            self._render_suggestions()
            event.prevent_default()
            event.stop()
        elif event.key in ("tab", "enter"):
            self._accept_current()
            event.prevent_default()
            event.stop()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-create":
            name = self.query_one("#ws-name", Input).value.strip()
            path = self.query_one("#ws-path", Input).value.strip()
            if name and path:
                self.dismiss(
                    Workspace(name=name, root_path=str(Path(path).expanduser()))
                )
            else:
                self.notify("Name and path are required.", severity="error")
        else:
            self.dismiss(None)

    def action_cancel(self) -> None:
        self.dismiss(None)
