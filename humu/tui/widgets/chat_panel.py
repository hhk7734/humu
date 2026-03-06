from __future__ import annotations

from textual.app import ComposeResult
from textual.containers import Vertical, VerticalScroll
from textual.events import Click
from textual.message import Message
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Static


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

    def __init__(self, sender: str) -> None:
        super().__init__()
        self._sender = sender
        self._spin_index = 0

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
    ChatPanel #chat-input {
        dock: bottom;
        margin: 0 1;
    }
    """

    def __init__(self) -> None:
        super().__init__()
        self._room_name: str | None = None

    def compose(self) -> ComposeResult:
        yield Label("Chat", classes="panel-title")
        with VerticalScroll(id="chat-scroll"):
            yield Vertical(id="chat-messages")
        yield Input(placeholder="Type a message...", id="chat-input")

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

    def show_loading(self, agent_name: str) -> None:
        self.hide_loading()
        container = self.query_one("#chat-messages", Vertical)
        loading = LoadingChatMessage(agent_name)
        loading.id = "loading-message"
        container.mount(loading)
        self.call_after_refresh(self._scroll_to_end)

    def hide_loading(self) -> None:
        try:
            self.query_one("#loading-message", LoadingChatMessage).remove()
        except Exception:
            pass

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

    def on_input_submitted(self, event: Input.Submitted) -> None:
        text = event.value.strip()
        if text:
            event.input.value = ""
            self.post_message(MessageSubmitted(text))
