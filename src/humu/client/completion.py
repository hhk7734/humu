from __future__ import annotations

import glob as _glob
import os
from collections import deque

from rich.cells import cell_len
from rich.text import Text


def fuzzy_match(needle: str, haystack: str) -> bool:
    """Case-insensitive subsequence match."""
    if not needle:
        return True
    it = iter(haystack.lower())
    return all(c in it for c in needle.lower())


def _path_needle(path: str) -> str:
    """Normalize path to lowercase for subsequence matching."""
    return path.lower().replace(os.sep, "")


def list_paths(
    base_dir: str,
    partial: str,
    max_results: int = 15,
    max_depth: int = 4,
) -> list[str]:
    """List filesystem paths matching partial via prefix glob + subsequence BFS."""
    if not os.path.isdir(base_dir):
        return []

    if not partial:
        try:
            matches = sorted(_glob.glob(os.path.join(base_dir, "*")))[:max_results]
        except Exception:
            return []
        result = []
        for m in matches:
            rel = os.path.relpath(m, base_dir)
            if os.path.isdir(m):
                rel += "/"
            result.append(rel)
        return result

    seen: set[str] = set()
    result: list[str] = []

    def _add(full_path: str) -> None:
        rel = os.path.relpath(full_path, base_dir)
        if rel in seen:
            return
        seen.add(rel)
        if os.path.isdir(full_path):
            rel += "/"
        result.append(rel)

    # Phase 1: prefix glob
    try:
        for m in sorted(_glob.glob(os.path.join(base_dir, partial + "*"))):
            if len(result) >= max_results:
                return result
            _add(m)
    except Exception:
        pass

    if len(result) >= max_results:
        return result

    # Phase 2: subsequence BFS
    needle = _path_needle(partial)
    if needle:
        q: deque[tuple[str, int]] = deque([(base_dir, 0)])
        while q and len(result) < max_results:
            cur_dir, depth = q.popleft()
            try:
                entries = sorted(
                    os.scandir(cur_dir),
                    key=lambda e: (not e.is_dir(follow_symlinks=False), e.name),
                )
            except OSError:
                continue
            for entry in entries:
                if len(result) >= max_results:
                    break
                rel = os.path.relpath(entry.path, base_dir)
                if _path_needle(rel) != needle and fuzzy_match(
                    needle, _path_needle(rel)
                ):
                    _add(entry.path)
                if entry.is_dir(follow_symlinks=False) and depth < max_depth:
                    q.append((entry.path, depth + 1))

    return result


def render_dropdown(
    items: list[str],
    selected_index: int,
    width: int,
    num_lines: int = 5,
) -> Text:
    """Render a dropdown display as Rich Text with selection highlight."""
    total = len(items)
    content = Text()

    if total == 0:
        for i in range(num_lines):
            if i > 0:
                content.append("\n")
        return content

    start = max(0, min(selected_index - num_lines // 2, total - num_lines))
    end = min(start + num_lines, total)
    rendered = 0

    for i in range(start, end):
        if rendered > 0:
            content.append("\n")
        path = items[i]
        is_selected = i == selected_index
        raw = f" > {path} " if is_selected else f"   {path}"
        if width > 0 and cell_len(raw) > width:
            clipped, w = "", 0
            for ch in raw:
                cw = cell_len(ch)
                if w + cw > width - 3:
                    break
                clipped += ch
                w += cw
            raw = clipped + "..."
        content.append(raw, style="bold reverse" if is_selected else "")
        rendered += 1

    while rendered < num_lines:
        content.append("\n")
        rendered += 1

    return content
