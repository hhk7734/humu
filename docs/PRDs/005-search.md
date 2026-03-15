# Search

Terminal pane search, adopted from Zellij's search model. Always uses regex (plain text works as substring match). No j/k keys — arrow keys only.

## Modes

Two new modes join the existing modal system:

| Mode | Purpose | Entry | Exit |
|------|---------|-------|------|
| EnterSearch | Type the search query | `Ctrl+f` from Terminal | `Enter` (confirm) / `Esc` (cancel) |
| Search | Navigate matches | `Enter` from EnterSearch | `Esc` → Terminal |

Flow: `Terminal` → `Ctrl+f` → **EnterSearch** → `Enter` → **Search** → `Esc` → `Terminal`

Canceling (`Esc`) from either mode clears search state and returns to Terminal. `Ctrl+w/r/t/p` mode switches work from both search modes via `check_mode_switch` — switching modes abandons search and clears search state (no "return to search" stack).

Empty query + `Enter` in EnterSearch is treated as cancel (returns to Terminal).

### Mode Badge

| Mode | Label | Color |
|------|-------|-------|
| EnterSearch | `SEARCH` | `accent_cyan` (`#56d4dd`) |
| Search | `SEARCH` | `accent_cyan` (`#56d4dd`) |

Add `accent_cyan: Color::Rgb(86, 212, 221)` to `Palette::GITHUB_DARK` and extend `Palette::mode_color` to return `accent_cyan` for both `EnterSearch` and `Search`.

## Keybindings

### EnterSearch Mode

| Key | Action |
|-----|--------|
| Printable char | Append to query, re-search live |
| `Backspace` | Delete last char, re-search |
| `Enter` | Confirm query, enter Search mode (cancel if empty) |
| `Esc` | Cancel, clear search, back to Terminal |

### Search Mode

| Key | Action |
|-----|--------|
| `n` | Next match |
| `N` (Shift+n) | Previous match |
| `c` | Toggle case sensitivity |
| `w` | Toggle wrap-around |
| `↑/↓` | Scroll up/down (1 line) |
| `PageUp/PageDown` | Page scroll |
| `Esc` | Exit search, clear highlights, back to Terminal |

## Search Engine

Always regex via the `regex` crate. Plain text input works as-is (literal substring match). Invalid regex patterns display an error indicator in the status bar and produce no matches (no panic).

### Options

| Option | Default | Toggle |
|--------|---------|--------|
| Case sensitivity | case-sensitive | `c` in Search mode |
| Wrap-around | off | `w` in Search mode |

Case-insensitive mode prepends `(?i)` to the pattern.

### Text Extraction

The vt100 viewport is fixed at `screen_height` rows. To access the full content (scrollback + viewport), the search scans in viewport-sized chunks:

1. Probe scrollback depth with a single brief lock on the live parser: call `parser.set_scrollback(usize::MAX)`, read `parser.screen().scrollback()` as `max_offset`, then call `parser.set_scrollback(original_offset)` to restore state. Release the lock immediately.
2. For each scrollback offset from `max_offset` down to 0 in steps of `screen_height`: lock the parser, call `set_scrollback(offset)`, clone the screen via `parser.screen().clone()`, restore the offset, release the lock.
3. On each cloned screen: iterate `screen.cell(row, col)` to extract text.
4. At each offset, iterate `screen.cell(row, col)` for each row/col to build a per-row string while tracking column positions. This preserves accurate column indices for wide characters and multi-byte Unicode.
5. Run the regex over each row string. Map byte offsets from regex matches back to column indices using the column tracking from step 4.
6. Store results as `SearchMatch` with absolute row coordinates (row 0 = oldest scrollback line). Given `max_offset` from step 1 and chunk `offset`, absolute row = `(max_offset - offset) + screen_row_index`. The reverse mapping for navigation: `target_offset = max_offset - absolute_row + screen_height / 2`, clamped to `[0, max_offset]`.

**Why cell-by-cell, not `screen.rows()`**: `screen.rows(start, width)` returns plain `String` per row, but byte offsets in those strings diverge from column indices when wide characters (CJK, emoji) are present. Cell iteration gives exact column positions.

### Match Navigation

When jumping to a match (`n`/`N`), adjust the pane's scrollback offset to center the match row in the viewport. Wrap-around jumps from the last match back to the first (and vice versa) when enabled; without wrap, navigation stops at the first/last match.

## Data Model

```rust
pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub active_index: Option<usize>,
    pub case_sensitive: bool,
    pub wrap: bool,
}

pub struct SearchMatch {
    /// Absolute row (0 = oldest scrollback line).
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
}
```

`SearchState` lives on `App`, not per-pane. It is cleared when:
- Exiting search (`Esc`)
- Switching modes via `Ctrl+w/r/t/p` (abandons search)
- Switching panes
- Switching rooms or workspaces (suspended panes may receive new output, invalidating match coordinates)

## Rendering

### Status Bar

**EnterSearch mode**: Replace the normal hint segments with a search input bar.

```
[SEARCH]▸ / query_text█
```

The cursor (`█`) follows the query text. Error indicator if regex is invalid:

```
[SEARCH]▸ / query_text  [invalid regex]
```

**Search mode**: Show query, option toggles, and match count.

```
[SEARCH]▸ / query_text  n NEXT  N PREV  c CASE  w WRAP  3/17
```

`3/17` = active match index / total matches. If no matches: `0/0`.

### Match Highlighting

During terminal pane rendering, after normal cell rendering, overlay match highlights on cells that fall within a `SearchMatch`:

- **Active match**: yellow background (`accent_yellow`), black foreground
- **Inactive matches**: dimmer highlight — `accent_yellow` at reduced intensity, preserving original foreground

Only matches visible in the current viewport are highlighted (skip off-screen matches). The existing `↑N` scrollback indicator in the pane border continues to display normally during search — it shows the scrollback position, which is useful context.

### Scrollback Integration

Search highlights work with the existing scrollback system. The vt100 scrollback offset determines which rows are visible, and `screen.cell(row, col)` already returns the correct cells for the current offset. Match row coordinates (absolute) are translated to viewport-relative coordinates for highlighting by subtracting the current scrollback base row.
