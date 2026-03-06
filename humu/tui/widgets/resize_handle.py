from __future__ import annotations

from textual import events
from textual.widget import Widget
from rich.text import Text
from rich.align import Align


class ResizeHandle(Widget):
    """A draggable 1-cell-wide vertical separator that resizes an adjacent panel.

    Args:
        target_id: CSS id of the panel to resize.
        min_width:  Minimum width the target panel may shrink to.
        invert:     If True, dragging *right* shrinks the target (used for
                    the handle placed to the *right* of the chat panel so
                    that dragging left enlarges the agent panel).
        save_callback: Called with (target_id, new_width) after each drag ends.
    """

    GLYPH = "⠿"  # braille drag-handle glyph

    DEFAULT_CSS = """
    ResizeHandle {
        width: 1;
        height: 100%;
        background: $panel-darken-1;
        color: $text-muted;
    }
    ResizeHandle:hover {
        background: $accent 40%;
        color: $accent;
    }
    """

    def render(self) -> Align:
        return Align(Text(self.GLYPH), align="center", vertical="middle")

    def __init__(
        self,
        target_id: str,
        *,
        min_width: int = 8,
        invert: bool = False,
        save_callback: "((str, int) -> None) | None" = None,
        **kwargs: object,
    ) -> None:
        super().__init__(**kwargs)
        self._target_id = target_id
        self._min_width = min_width
        self._invert = invert
        self._save_callback = save_callback
        self._drag_start_x: int | None = None
        self._drag_origin_width: int = 0
        self._last_set_width: int | None = None  # avoid redundant layout refreshes

    def on_mouse_down(self, event: events.MouseDown) -> None:
        self.capture_mouse()
        self._drag_start_x = event.screen_x
        target = self.app.query_one(f"#{self._target_id}")
        self._drag_origin_width = target.size.width
        self._last_set_width = self._drag_origin_width
        event.stop()

    def on_mouse_move(self, event: events.MouseMove) -> None:
        if self._drag_start_x is None:
            return
        delta = event.screen_x - self._drag_start_x
        if self._invert:
            delta = -delta
        new_width = max(self._min_width, self._drag_origin_width + delta)
        # Skip the style update (and resulting layout refresh) when width hasn't changed
        if new_width == self._last_set_width:
            event.stop()
            return
        self._last_set_width = new_width
        target = self.app.query_one(f"#{self._target_id}")
        target.styles.width = new_width
        event.stop()

    def on_mouse_up(self, event: events.MouseUp) -> None:
        if self._drag_start_x is None:
            return
        self.release_mouse()
        self._drag_start_x = None
        target = self.app.query_one(f"#{self._target_id}")
        if self._save_callback:
            self._save_callback(self._target_id, target.size.width)
        self._last_set_width = None
        event.stop()
