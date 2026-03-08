# Autocomplete

Inline autocomplete for file paths and skills across the TUI — in workspace creation modals and the chat input.

## Triggers

- `@` — file path autocomplete. In chat input only.
- `/` — skill autocomplete. In chat input only, at position 0.
- Plain typing — path autocomplete in the workspace creation root path input (no trigger character).

## Path Resolution

Paths resolve differently depending on context:

**Workspace root path input (modal):**
- Starts with `/` → absolute path
- Starts with `~/` → expands to home directory
- Otherwise → relative to `~/` (home)

**Chat input (`@` trigger):**
- `@/` → absolute path
- `@~/` → expands to home directory
- `@` without prefix → relative to current workspace root (falls back to `~/` if no workspace selected)

## Matching

**Path matching** — two-phase:
1. **Prefix glob** in the target directory. If input is `/home/user/pro`, glob `pro*` in `/home/user/`. If input ends with `/`, list directory contents.
2. **Subsequence BFS** — if prefix glob yields fewer than 15 results, walk the directory tree breadth-first (max depth 4) and fuzzy-match each entry. A subsequence match means each character of the query appears in order in the candidate (case-insensitive).

Max 15 results.

**Hidden files** — entries starting with `.` (e.g., `.git/`, `.venv/`, `.env`) are excluded from directory listings and BFS traversal. They are still matched by prefix glob when the user explicitly types a dot prefix (e.g., `.g` matches `.git/`).

**Skill matching** — fuzzy subsequence on the full skill name (`marketplace:skill-name`) and on the skill part after `:`. Shows skill name + description in the dropdown.

## Dropdown Display

Both completers render an overlay dropdown — 5 visible lines, using `position: absolute` on an overlay layer. The dropdown floats on top of other widgets without affecting layout or resizing the modal.

- **PathInputCompleter** (modal): drops **down** below the input.
- **ChatCompleter** (chat): drops **up** above the input.

Each line shows one suggestion. The selected item is highlighted with `bold reverse` style and a `>` prefix. Long entries are clipped with `...`.

## Keyboard Interaction

### PathInputCompleter (workspace creation modal)

| Key | Action |
|-----|--------|
| Any typing | Refresh suggestions |
| Arrow Down / Up | Navigate suggestions |
| Enter | Accept selection, replace input value, keep completer open (refreshes for new path) |
| Tab | Move focus to next widget (default behavior), close completer |
| Escape | Close completer if open, dismiss modal if closed |

When focus leaves the root path input (Tab, click), the completer closes automatically.

### ChatCompleter (chat input)

| Key | Action |
|-----|--------|
| `@` or `/` (at pos 0) | Open completer |
| Typing after trigger | Filter suggestions |
| Arrow Down / Up | Navigate suggestions (reversed visually since drop-up) |
| Tab | Accept selection, insert into text, keep completer open (refreshes) |
| Enter | Accept selection, insert into text + trailing space, close completer. If no completer visible, send message. |
| Escape | Close completer, keep trigger text |
| Backspace past trigger | Close completer |
| Cursor moves before trigger | Close completer |

Focus stays on the Input/TextArea at all times. Arrow keys are intercepted only when the completer is visible and has results.

## Widget Architecture

Two separate completer widgets with shared utility functions.

**Widgets** (`src/humu/client/completers.py`):
- `PathInputCompleter` — attaches to an `Input` widget, drops down, query = full input value.
- `ChatCompleter` — attaches to the `TextArea` (`#chat-input`), drops up, trigger-based (`@` paths, `/` skills).

**Shared helpers** (`src/humu/client/completion.py`):
- `list_paths(base_dir, partial, max_results=15, max_depth=4) -> list[str]` — prefix glob + subsequence BFS.
- `fuzzy_match(needle, haystack) -> bool` — case-insensitive subsequence check.
- `render_dropdown(items, selected_index, width, num_lines=5) -> Rich Text` — renders the 5-line display with selection highlight and clipping.

## Skill Source

`/` completes plugin skills only (from `PluginManager.list_skills()`). No builtin commands for now — extensible later.
