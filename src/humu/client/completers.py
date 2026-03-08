from __future__ import annotations

import os
from pathlib import Path

from textual.widgets import Static

from humu.client.completion import list_paths, render_dropdown


class PathInputCompleter(Static):
    """Dropdown completer for Input widgets. Drops down, overlay layer."""

    DEFAULT_CSS = """
    PathInputCompleter {
        height: 5;
        layer: overlay;
        display: none;
    }
    PathInputCompleter.visible {
        display: block;
        background: $surface;
        border: tall $accent;
        padding: 0 1;
    }
    """

    def __init__(self, input_id: str, **kwargs) -> None:
        super().__init__("", **kwargs)
        self._input_id = input_id
        self._items: list[str] = []
        self._index: int = 0
        self._base_dir: str = ""

    @property
    def is_active(self) -> bool:
        return bool(self._items)

    @property
    def selected(self) -> str | None:
        if self._items and 0 <= self._index < len(self._items):
            return self._items[self._index]
        return None

    def move_down(self) -> None:
        if self._items:
            self._index = (self._index + 1) % len(self._items)
            self._refresh()

    def move_up(self) -> None:
        if self._items:
            self._index = (self._index - 1) % len(self._items)
            self._refresh()

    def accept(self) -> str | None:
        """Accept current selection, return the full path string."""
        sel = self.selected
        if sel is None:
            return None
        full = os.path.join(self._base_dir, sel)
        return full

    def hide(self) -> None:
        self._items = []
        self._index = 0
        self.remove_class("visible")
        self.update("")

    def refresh_completions(self, text: str) -> None:
        """Compute completions for the given input text."""
        expanded = os.path.expanduser(text)
        if text.startswith("/") or text.startswith("~/"):
            # Absolute or home-relative
            if "/" in expanded and not expanded.endswith("/"):
                base = os.path.dirname(expanded)
                partial = os.path.basename(expanded)
            elif expanded.endswith("/"):
                base = expanded
                partial = ""
            else:
                base = expanded
                partial = ""
        else:
            # Relative to ~/
            home = str(Path.home())
            full = os.path.join(home, expanded)
            if expanded and "/" in expanded and not expanded.endswith("/"):
                base = os.path.dirname(full)
                partial = os.path.basename(full)
            elif expanded.endswith("/"):
                base = full
                partial = ""
            else:
                base = home
                partial = expanded

        self._base_dir = base
        paths = list_paths(base, partial)

        if paths:
            self._items = paths
            self._index = 0
            self.add_class("visible")
            self._refresh()
        else:
            self.hide()

    def _refresh(self) -> None:
        width = self.content_size.width or 50
        text = render_dropdown(self._items, self._index, width, num_lines=5)
        self.update(text)
