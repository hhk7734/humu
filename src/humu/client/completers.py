from __future__ import annotations

import os
from collections.abc import Callable
from pathlib import Path

from textual.widgets import Static

from humu.client.completion import fuzzy_match, list_paths, render_dropdown


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


class ChatCompleter(Static):
    """Dropdown completer for chat TextArea. Drops up, overlay layer.
    Triggers: @ for paths, / for skills (at position 0 only).
    """

    DEFAULT_CSS = """
    ChatCompleter {
        height: 5;
        layer: overlay;
        dock: bottom;
        display: none;
        offset: 0 -8;
    }
    ChatCompleter.visible {
        display: block;
        background: $surface;
        border: tall $accent;
        padding: 0 1;
    }
    """

    def __init__(self, **kwargs) -> None:
        super().__init__("", **kwargs)
        self._items: list[str] = []
        self._raw_items: list[str] = []
        self._index: int = 0
        self._trigger_char: str = ""
        self._trigger_start: int = -1
        self._workspace_root: str | None = None
        self._get_skills: Callable[[], list[dict]] | None = None

    @property
    def is_active(self) -> bool:
        return bool(self._items)

    @property
    def trigger_char(self) -> str:
        return self._trigger_char

    @property
    def trigger_start(self) -> int:
        return self._trigger_start

    def set_workspace_root(self, root: str | None) -> None:
        self._workspace_root = root

    def set_skill_provider(self, provider: Callable[[], list[dict]]) -> None:
        self._get_skills = provider

    @property
    def selected(self) -> str | None:
        if self._raw_items and 0 <= self._index < len(self._raw_items):
            return self._raw_items[self._index]
        return None

    def move_down(self) -> None:
        if self._items:
            self._index = (self._index + 1) % len(self._items)
            self._refresh()

    def move_up(self) -> None:
        if self._items:
            self._index = (self._index - 1) % len(self._items)
            self._refresh()

    def hide(self) -> None:
        self._items = []
        self._raw_items = []
        self._index = 0
        self._trigger_char = ""
        self._trigger_start = -1
        self.remove_class("visible")
        self.update("")

    def update_completions(self, text: str, cursor_pos: int) -> None:
        """Recompute completions based on current text and cursor position."""
        trigger_char = ""
        trigger_start = -1

        # Check for / at position 0
        if text.startswith("/"):
            trigger_char = "/"
            trigger_start = 0
        else:
            # Scan backwards from cursor for @
            for i in range(cursor_pos - 1, -1, -1):
                ch = text[i]
                if ch == "@":
                    trigger_char = "@"
                    trigger_start = i
                    break
                if ch in (" ", "\n"):
                    break

        if not trigger_char:
            self.hide()
            return

        self._trigger_char = trigger_char
        self._trigger_start = trigger_start
        query = text[trigger_start + 1 : cursor_pos]

        if trigger_char == "@":
            self._complete_paths(query)
        elif trigger_char == "/":
            self._complete_skills(query)

    def _complete_paths(self, query: str) -> None:
        expanded = os.path.expanduser(query) if query else ""

        if query.startswith("/"):
            # Absolute
            if "/" in expanded[1:] and not expanded.endswith("/"):
                base = os.path.dirname(expanded)
                partial = os.path.basename(expanded)
            elif expanded.endswith("/"):
                base = expanded
                partial = ""
            else:
                base = "/"
                partial = expanded.lstrip("/")
        elif query.startswith("~/"):
            # Home-relative
            home_expanded = os.path.expanduser("~/")
            rest = expanded[len(home_expanded):]
            if "/" in rest and not expanded.endswith("/"):
                base = os.path.dirname(expanded)
                partial = os.path.basename(expanded)
            elif expanded.endswith("/"):
                base = expanded
                partial = ""
            else:
                base = os.path.expanduser("~")
                partial = query[2:]
        else:
            # Relative to workspace root
            root = self._workspace_root or str(Path.home())
            if query and "/" in query and not query.endswith("/"):
                full = os.path.join(root, query)
                base = os.path.dirname(full)
                partial = os.path.basename(full)
            elif query.endswith("/"):
                base = os.path.join(root, query)
                partial = ""
            else:
                base = root
                partial = query

        paths = list_paths(base, partial)
        if paths:
            self._items = paths
            self._raw_items = paths
            self._index = 0
            self.add_class("visible")
            self._refresh()
        else:
            self.hide()

    def _complete_skills(self, query: str) -> None:
        if self._get_skills is None:
            self.hide()
            return

        skills = self._get_skills()
        display_items = []
        raw_items = []

        for s in skills:
            name = s.get("name", "")
            desc = s.get("description", "")
            skill_part = name.split(":", 1)[1] if ":" in name else name
            if not query or fuzzy_match(query, name) or fuzzy_match(query, skill_part):
                label = f"{name}  {desc}" if desc else name
                display_items.append(label)
                raw_items.append(name)

        if display_items:
            self._items = display_items
            self._raw_items = raw_items
            self._index = 0
            self.add_class("visible")
            self._refresh()
        else:
            self.hide()

    def _refresh(self) -> None:
        width = self.content_size.width or 50
        text = render_dropdown(self._items, self._index, width, num_lines=5)
        self.update(text)
