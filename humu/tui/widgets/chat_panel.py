from __future__ import annotations

import glob as _glob
import os
from collections import deque
from collections.abc import Callable

from textual.app import ComposeResult
from textual.containers import Vertical, VerticalScroll
from textual.events import Click, Key
from textual.message import Message
from textual.screen import ModalScreen
from textual.widgets import Button, Label, Static, TextArea


def _path_needle(text: str) -> str:
    """Normalise a path/partial for subsequence matching (lowercase, strip separators/punctuation)."""
    return text.lower().replace(os.sep, "").replace("/", "").replace("-", "").replace("_", "")


def _is_subsequence(needle: str, haystack: str) -> bool:
    """Return True if every character of *needle* appears in *haystack* in order."""
    it = iter(haystack)
    return all(ch in it for ch in needle)


class MessageSubmitted(Message):
    def __init__(self, text: str) -> None:
        super().__init__()
        self.text = text


class LoadingChatMessage(Vertical):
    """Animated loading indicator shown as a chat message while an agent is thinking."""

    DEFAULT_CSS = """
    LoadingChatMessage {
        padding: 0 1;
        margin: 0 0 1 0;
        height: auto;
        width: 1fr;
    }
    LoadingChatMessage .sender {
        text-style: bold;
        color: $accent;
        width: 1fr;
        height: auto;
    }
    LoadingChatMessage .loading-text {
        padding: 0 0 0 2;
        width: 1fr;
        height: auto;
        color: $text-muted;
        text-style: italic;
    }
    """

    SPINNERS = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

    def __init__(self, sender: str, get_steps: Callable[[], list[dict]] | None = None) -> None:
        super().__init__()
        self._sender = sender
        self._spin_index = 0
        self._get_steps = get_steps

    def compose(self) -> ComposeResult:
        from rich.text import Text

        yield Label(Text(f"[{self._sender}]"), classes="sender")
        yield Static(f"{self.SPINNERS[0]} thinking...", classes="loading-text")

    def on_mount(self) -> None:
        self._timer = self.set_interval(0.1, self._tick)

    def _tick(self) -> None:
        self._spin_index = (self._spin_index + 1) % len(self.SPINNERS)
        spinner = self.SPINNERS[self._spin_index]
        self.query_one(".loading-text", Static).update(f"{spinner} thinking...")

    def on_click(self, event: Click) -> None:
        if event.button == 3 and self._get_steps:  # right-click
            from humu.tui.screens.message_detail import MessageDetailScreen
            steps = self._get_steps()
            self.app.push_screen(
                MessageDetailScreen(self._sender, "", steps=steps)
            )


class MessageContextMenu(ModalScreen[None]):
    """Right-click context menu for a chat message."""

    BINDINGS = [("escape", "dismiss", "Close")]

    DEFAULT_CSS = """
    MessageContextMenu {
        align: center middle;
        background: $background 50%;
    }
    MessageContextMenu #menu {
        width: 24;
        height: auto;
        border: solid $accent;
        background: $surface;
        padding: 0;
    }
    MessageContextMenu Button {
        width: 1fr;
        background: transparent;
        border: none;
        text-align: left;
        padding: 0 1;
    }
    MessageContextMenu Button:hover {
        background: $accent 30%;
    }
    """

    def __init__(
        self,
        sender: str,
        text: str,
        is_system: bool,
        raw: str | None,
        steps: list[dict] | None = None,
    ) -> None:
        super().__init__()
        self._sender = sender
        self._text = text
        self._is_system = is_system
        self._raw = raw
        self._steps = steps or []

    def compose(self) -> ComposeResult:
        with Vertical(id="menu"):
            yield Button("View Details", id="btn-details")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-details":
            from humu.tui.screens.message_detail import MessageDetailScreen

            self.dismiss()
            self.app.push_screen(
                MessageDetailScreen(
                    self._sender, self._text, self._is_system,
                    raw=self._raw, steps=self._steps,
                )
            )

    def on_click(self, event: Click) -> None:
        menu = self.query_one("#menu", Vertical)
        if not menu.region.contains(event.screen_x, event.screen_y):
            self.dismiss()


class ChatInput(TextArea):
    """TextArea that submits on Enter and inserts newline on Shift+Enter.

    Set ``suppress_enter = True`` while an autocomplete menu is active so
    that Enter selects the completion instead of submitting the message.
    """

    class Submitted(Message):
        def __init__(self, text: str) -> None:
            super().__init__()
            self.text = text

    suppress_enter: bool = False
    # Set by ChatPanel to enable history navigation before TextArea processes arrows
    on_history_up: "Callable[[], bool] | None" = None
    on_history_down: "Callable[[], bool] | None" = None

    def _on_key(self, event: Key) -> None:
        if event.key == "enter":
            if self.suppress_enter:
                # Prevent TextArea default (newline insertion) but let the
                # event bubble up to ChatPanel.on_key for autocomplete selection
                event.prevent_default()
                return
            text = self.text.strip()
            if text:
                self.load_text("")
                self.post_message(self.Submitted(text))
            event.prevent_default()
            event.stop()
            return
        if event.key == "shift+enter":
            self.insert("\n")
            event.prevent_default()
            event.stop()
            return
        # History navigation — intercept before TextArea's cursor-movement bindings
        if event.key == "up" and self.on_history_up and self.on_history_up():
            event.prevent_default()
            event.stop()
            return
        if event.key == "down" and self.on_history_down and self.on_history_down():
            event.prevent_default()
            event.stop()
            return
        super()._on_key(event)


class PathAutocomplete(Static):
    """Fixed 3-line area showing path/skill completions below the input border."""

    DEFAULT_CSS = """
    PathAutocomplete {
        height: 3;
        margin: 0 1;
        padding: 0 1;
        color: $text-muted;
    }
    PathAutocomplete.active {
        background: $surface;
        border: solid $accent;
        color: $text;
    }
    """

    def __init__(self, **kwargs: object) -> None:
        super().__init__("", **kwargs)
        self._paths: list[str] = []
        self._index: int = 0

    @property
    def is_active(self) -> bool:
        return bool(self._paths)

    def show_paths(self, paths: list[str]) -> None:
        self._paths = paths
        self._index = 0
        self._refresh_display()
        self.add_class("active")

    def clear(self) -> None:
        self._paths = []
        self._index = 0
        self.update("\n\n")  # keep 3 lines height
        self.remove_class("active")

    def move_down(self) -> None:
        if self._paths:
            self._index = (self._index + 1) % len(self._paths)
            self._refresh_display()

    def move_up(self) -> None:
        if self._paths:
            self._index = (self._index - 1) % len(self._paths)
            self._refresh_display()

    def current_path(self) -> str | None:
        if self._paths and 0 <= self._index < len(self._paths):
            return self._paths[self._index]
        return None

    def _refresh_display(self) -> None:
        total = len(self._paths)
        window = 3
        start = max(0, min(self._index - window // 2, total - window))
        end = min(start + window, total)
        lines = []
        for i in range(start, end):
            path = self._paths[i]
            if i == self._index:
                lines.append(f"[bold reverse] ❯ {path} [/bold reverse]")
            else:
                lines.append(f"   {path}")
        # Pad to always fill 3 lines
        while len(lines) < 3:
            lines.append("")
        self.update("\n".join(lines))


class ChatMessage(Vertical):
    DEFAULT_CSS = """
    ChatMessage {
        padding: 0 1;
        margin: 0 0 1 0;
        height: auto;
        width: 1fr;
    }
    ChatMessage .sender {
        text-style: bold;
        color: $accent;
        width: 1fr;
        height: auto;
    }
    ChatMessage .sender-system {
        text-style: italic;
        color: $text-muted;
        width: 1fr;
        height: auto;
    }
    ChatMessage .sender-error {
        text-style: bold;
        color: $error;
        width: 1fr;
        height: auto;
    }
    ChatMessage .msg-text {
        padding: 0 0 0 2;
        width: 1fr;
        height: auto;
    }
    """

    def __init__(
        self,
        sender: str,
        text: str,
        is_system: bool = False,
        raw: str | None = None,
        steps: list[dict] | None = None,
    ) -> None:
        super().__init__()
        self._sender = sender
        self._text = text
        self._is_system = is_system
        self._raw = raw
        self._steps = steps or []

    def compose(self) -> ComposeResult:
        from rich.text import Text

        if self._sender == "error":
            yield Label(Text(f"[{self._sender}]"), classes="sender-error")
        elif self._is_system:
            yield Label(Text(f"[{self._sender}]"), classes="sender-system")
        else:
            yield Label(Text(f"[{self._sender}]"), classes="sender")
        yield Static(self._text, classes="msg-text")

    def on_click(self, event: Click) -> None:
        if event.button == 3:  # right-click
            self.app.push_screen(
                MessageContextMenu(
                    self._sender, self._text, self._is_system,
                    raw=self._raw, steps=self._steps,
                )
            )


class ChatPanel(Static):
    DEFAULT_CSS = """
    ChatPanel {
        width: 1fr;
        height: 100%;
        border: solid $accent;
    }
    ChatPanel .panel-title {
        text-style: bold;
        padding: 0 1;
        background: $accent;
        color: $text;
    }
    ChatPanel #chat-scroll {
        height: 1fr;
    }
    ChatPanel #chat-messages {
        height: auto;
        width: 1fr;
    }
    ChatPanel #bottom-area {
        dock: bottom;
        height: auto;
        padding: 0;
    }
    ChatPanel #input-box {
        height: auto;
        border-top: solid $accent 50%;
        border-bottom: solid $accent 50%;
        padding: 0;
        background: $surface;
    }
    ChatPanel #chat-input {
        margin: 0 1;
        height: 3;
        max-height: 10;
        border: none;
    }
    ChatPanel #chat-input:focus {
        border: none;
    }
    ChatPanel #queue-display {
        margin: 0 1;
        padding: 0 1;
        height: auto;
        color: $text-muted;
        background: $surface;
        border-top: solid $accent 30%;
        display: none;
    }
    ChatPanel #queue-display.visible {
        display: block;
    }
    """

    def __init__(self, get_skills: Callable[[], list[dict]] | None = None) -> None:
        super().__init__()
        self._room_name: str | None = None
        self._workspace_path: str | None = None
        self._trigger_start: int = -1  # flat position of trigger char
        self._trigger_char: str = ""   # "@" or "/"
        self._get_skills = get_skills
        # Input history (Up/Down to recall previous messages)
        self._input_history: list[str] = []
        self._history_index: int = -1   # -1 = not navigating
        self._history_draft: str = ""   # saved draft while navigating history

    def compose(self) -> ComposeResult:
        yield Label("Chat", classes="panel-title")
        with VerticalScroll(id="chat-scroll"):
            yield Vertical(id="chat-messages")
        with Vertical(id="bottom-area"):
            with Vertical(id="input-box"):
                yield Static("", id="queue-display")
                yield ChatInput(id="chat-input", show_line_numbers=False)
            yield PathAutocomplete(id="path-autocomplete")

    def on_mount(self) -> None:
        textarea = self.query_one("#chat-input", ChatInput)
        textarea.focus()
        textarea.on_history_up = self._navigate_history_up
        textarea.on_history_down = self._navigate_history_down

    def _navigate_history_up(self) -> bool:
        """Navigate to the previous history entry. Returns True if handled."""
        autocomplete = self.query_one("#path-autocomplete", PathAutocomplete)
        if autocomplete.is_active or not self._input_history:
            return False
        textarea = self.query_one("#chat-input", ChatInput)
        if self._history_index == -1:
            self._history_draft = textarea.text
            self._history_index = len(self._input_history) - 1
        elif self._history_index > 0:
            self._history_index -= 1
        textarea.load_text(self._input_history[self._history_index])
        return True

    def _navigate_history_down(self) -> bool:
        """Navigate to the next history entry. Returns True if handled."""
        autocomplete = self.query_one("#path-autocomplete", PathAutocomplete)
        if autocomplete.is_active or self._history_index == -1:
            return False
        textarea = self.query_one("#chat-input", ChatInput)
        if self._history_index < len(self._input_history) - 1:
            self._history_index += 1
            textarea.load_text(self._input_history[self._history_index])
        else:
            self._history_index = -1
            textarea.load_text(self._history_draft)
            self._history_draft = ""
        return True

    def on_mouse_up(self) -> None:
        """Clicking or dragging anywhere in the chat panel refocuses the input."""
        self.call_after_refresh(lambda: self.query_one("#chat-input", ChatInput).focus())

    def set_pending_queue(self, messages: list[str]) -> None:
        """Show or hide the pending message queue above the input."""
        display = self.query_one("#queue-display", Static)
        if not messages:
            display.update("")
            display.remove_class("visible")
            return
        lines = []
        for i, msg in enumerate(messages, 1):
            preview = msg[:60] + "…" if len(msg) > 60 else msg
            lines.append(f"[dim]{i}.[/dim] {preview}")
        header = f"[bold]Queued ({len(messages)})[/bold]"
        display.update(header + "\n" + "\n".join(lines))
        display.add_class("visible")

    def set_workspace_path(self, path: str | None) -> None:
        self._workspace_path = path

    def set_room(self, name: str | None) -> None:
        self._room_name = name
        title = f"Chat - {name}" if name else "Chat"
        self.query_one(".panel-title", Label).update(title)

    def clear_messages(self) -> None:
        container = self.query_one("#chat-messages", Vertical)
        container.remove_children()

    def add_message(
        self,
        sender: str,
        text: str,
        is_system: bool = False,
        raw: str | None = None,
        steps: list[dict] | None = None,
    ) -> None:
        container = self.query_one("#chat-messages", Vertical)
        msg = ChatMessage(sender, text, is_system, raw=raw, steps=steps)
        container.mount(msg)
        self.call_after_refresh(self._scroll_to_end)

    def _scroll_to_end(self) -> None:
        scroll = self.query_one("#chat-scroll", VerticalScroll)
        scroll.scroll_end(animate=False)

    def show_loading(self, agent_name: str, get_steps: Callable[[], list[dict]] | None = None) -> None:
        self.hide_loading()
        container = self.query_one("#chat-messages", Vertical)
        loading = LoadingChatMessage(agent_name, get_steps=get_steps)
        container.mount(loading)
        self.call_after_refresh(self._scroll_to_end)

    def hide_loading(self) -> None:
        for widget in self.query(LoadingChatMessage):
            widget.remove()

    def load_history(self, messages: list[dict]) -> None:
        self.clear_messages()
        for msg in messages:
            self.add_message(
                msg["sender"],
                msg["text"],
                msg.get("is_system", False),
                raw=msg.get("raw"),
                steps=msg.get("steps"),
            )

    def _cursor_flat_pos(self, textarea: ChatInput) -> int:
        """Return the cursor position as a flat character index into textarea.text."""
        text = textarea.text
        row, col = textarea.cursor_location
        lines = text.split("\n")
        return sum(len(lines[i]) + 1 for i in range(row)) + col

    def on_text_area_changed(self, event: TextArea.Changed) -> None:
        textarea = self.query_one("#chat-input", ChatInput)
        text = textarea.text
        flat_pos = self._cursor_flat_pos(textarea)
        prefix = text[:flat_pos]
        autocomplete = self.query_one("#path-autocomplete", PathAutocomplete)

        # @ path trigger
        at_pos = prefix.rfind("@")
        if at_pos != -1:
            partial = prefix[at_pos + 1:]
            if " " not in partial and "\n" not in partial:
                self._trigger_start = at_pos
                self._trigger_char = "@"
                paths = self._list_paths(partial)
                autocomplete.show_paths(paths)
                textarea.suppress_enter = bool(paths)
                return

        # / skill trigger — only at start of text or after space/newline
        slash_pos = prefix.rfind("/")
        if slash_pos != -1:
            before = prefix[:slash_pos]
            if before == "" or before[-1] in (" ", "\n"):
                partial = prefix[slash_pos + 1:]
                if " " not in partial and "\n" not in partial:
                    self._trigger_start = slash_pos
                    self._trigger_char = "/"
                    skills = self._list_skills(partial)
                    autocomplete.show_paths(skills)
                    textarea.suppress_enter = bool(skills)
                    return

        autocomplete.clear()
        textarea.suppress_enter = False
        self._trigger_start = -1
        self._trigger_char = ""

    def _list_paths(self, partial: str) -> list[str]:
        if not self._workspace_path:
            return []
        base = self._workspace_path

        if not partial:
            # No input yet — show top-level entries only
            try:
                matches = sorted(_glob.glob(os.path.join(base, "*")))[:15]
            except Exception:
                return []
            result = []
            for m in matches:
                rel = os.path.relpath(m, base)
                if os.path.isdir(m):
                    rel += "/"
                result.append(rel)
            return result

        MAX_RESULTS = 15
        seen: set[str] = set()
        result: list[str] = []

        def _add(full_path: str) -> None:
            rel = os.path.relpath(full_path, base)
            if rel in seen:
                return
            seen.add(rel)
            if os.path.isdir(full_path):
                rel += "/"
            result.append(rel)

        # 1) Exact prefix glob (fast, highest priority)
        try:
            for m in sorted(_glob.glob(os.path.join(base, partial + "*"))):
                if len(result) >= MAX_RESULTS:
                    return result
                _add(m)
        except Exception:
            pass

        if len(result) >= MAX_RESULTS:
            return result

        # 2) Fuzzy subsequence search through subdirectory tree
        needle = _path_needle(partial)
        if needle:
            q: deque[tuple[str, int]] = deque([(base, 0)])
            while q and len(result) < MAX_RESULTS:
                cur_dir, depth = q.popleft()
                try:
                    entries = sorted(os.scandir(cur_dir), key=lambda e: (not e.is_dir(follow_symlinks=False), e.name))
                except OSError:
                    continue
                for entry in entries:
                    if len(result) >= MAX_RESULTS:
                        break
                    rel = os.path.relpath(entry.path, base)
                    if _is_subsequence(needle, _path_needle(rel)):
                        _add(entry.path)
                    if entry.is_dir(follow_symlinks=False) and depth < 4:
                        q.append((entry.path, depth + 1))

        return result

    def _list_skills(self, partial: str) -> list[str]:
        if not self._get_skills:
            return []
        results = []
        for s in self._get_skills():
            name = s.get("name", "")
            desc = s.get("description", "")
            if name.startswith(partial):
                label = f"{name}  {desc[:60]}" if desc else name
                results.append(label)
        return results

    def _apply_autocomplete(self, item: str) -> None:
        textarea = self.query_one("#chat-input", ChatInput)
        if self._trigger_start != -1:
            if self._trigger_char == "/":
                # Skill: strip description label, add trailing space
                insert = item.split("  ")[0] + " "
            elif self._trigger_char == "@":
                # Path: add trailing space only for files (dirs end with "/")
                insert = item if item.endswith("/") else item + " "
            else:
                insert = item
            text = textarea.text
            flat_pos = self._cursor_flat_pos(textarea)
            new_text = text[: self._trigger_start + 1] + insert + text[flat_pos:]
            textarea.load_text(new_text)
            new_cursor_flat = self._trigger_start + 1 + len(insert)
            new_lines = new_text.split("\n")
            offset = new_cursor_flat
            for row, line in enumerate(new_lines):
                if offset <= len(line):
                    textarea.move_cursor((row, offset))
                    break
                offset -= len(line) + 1
        textarea = self.query_one("#chat-input", ChatInput)
        textarea.suppress_enter = False
        self.query_one("#path-autocomplete", PathAutocomplete).clear()
        self._trigger_start = -1
        self._trigger_char = ""

    def on_chat_input_submitted(self, event: ChatInput.Submitted) -> None:
        textarea = self.query_one("#chat-input", ChatInput)
        textarea.suppress_enter = False
        self.query_one("#path-autocomplete", PathAutocomplete).clear()
        self._trigger_start = -1
        self._trigger_char = ""
        # Add to history (avoid consecutive duplicates)
        if event.text and (not self._input_history or self._input_history[-1] != event.text):
            self._input_history.append(event.text)
        self._history_index = -1
        self._history_draft = ""
        self.post_message(MessageSubmitted(event.text))

    def on_key(self, event: Key) -> None:
        autocomplete = self.query_one("#path-autocomplete", PathAutocomplete)

        if not autocomplete.is_active:
            return

        if event.key == "escape":
            textarea = self.query_one("#chat-input", ChatInput)
            textarea.suppress_enter = False
            autocomplete.clear()
            self._trigger_start = -1
            self._trigger_char = ""
            event.prevent_default()
            event.stop()
        elif event.key == "down":
            autocomplete.move_down()
            event.prevent_default()
        elif event.key == "up":
            autocomplete.move_up()
            event.prevent_default()
        elif event.key == "tab":
            path = autocomplete.current_path()
            if path:
                self._apply_autocomplete(path)
            event.prevent_default()
            event.stop()
        elif event.key == "enter":
            # Enter selects from autocomplete (both @ and /) without submitting
            path = autocomplete.current_path()
            if path:
                self._apply_autocomplete(path)
            event.prevent_default()
            event.stop()
