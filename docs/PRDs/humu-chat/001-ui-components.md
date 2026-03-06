# Humu — UI Components Reference

This document names and describes every UI component in Humu so that developers and contributors can refer to them unambiguously.

## Main Layout

```
+──────────────────────────────────────── Header ─────────────────────────────────────────+
│                  │   │           │   │                           │   │                  │
│  WorkspacePanel  │⠿  │ RoomPanel │⠿  │         ChatPanel         │⠿  │   AgentPanel     │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│  > my-app    ⠹   │   │ > design  │   │  │   #chat-messages    │  │   │  * leader        │
│    infra         │   │   dev     │   │  │                     │  │   │    opus          │
│    docs          │   │   review  │   │  │  [you] How should   │  │   │    backend       │
│                  │   │           │   │  │  we structure this? │  │   │    sonnet        │
│                  │   │           │   │  │                     │  │   │    security      │
│                  │   │           │   │  │  [leader] Routing…  │  │   │    haiku         │
│                  │   │           │   │  │                     │  │   │                  │
│                  │   │           │   │  │  [backend] I would  │  │   │                  │
│                  │   │           │   │  │  recommend REST…    │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│                  │   │           │   │  │   #queue-display    │  │   │                  │
│                  │   │           │   │  │  Queued (1) …       │  │   │                  │
│                  │   │           │   │  ├─────────────────────┤  │   │                  │
│                  │   │           │   │  │     #chat-input     │  │   │                  │
│                  │   │           │   │  │  > your message…    │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
│                  │   │           │   │  ┌─────────────────────┐  │   │                  │
│                  │   │           │   │  │  #path-autocomplete │  │   │                  │
│                  │   │           │   │  │   ❯ src/            │  │   │                  │
│                  │   │           │   │  └─────────────────────┘  │   │                  │
+──────────────────────────────────────── Footer ─────────────────────────────────────────+
```

The horizontal layout is composed of four panels separated by three `ResizeHandle` widgets (`⠿`). All panels fill the full height between `Header` and `Footer`. Inside `ChatPanel`, the `#chat-messages` area scrolls vertically; `#queue-display`, `#chat-input`, and `#path-autocomplete` are docked to the bottom.

---

## Permanent Components

### Header

| Property | Value                                      |
| :------- | :----------------------------------------- |
| Widget   | `textual.widgets.Header`                   |
| Content  | Application title "Humu" and a "Menu" icon |

Textual built-in top bar. Displays the app title and the current theme-aware icon.

---

### Footer

| Property | Value                    |
| :------- | :----------------------- |
| Widget   | `textual.widgets.Footer` |

Textual built-in bottom bar. Automatically renders the active key bindings defined in `HumuApp.BINDINGS`.

---

### WorkspacePanel

| Property      | Value                                             |
| :------------ | :------------------------------------------------ |
| Class         | `humu.tui.widgets.workspace_panel.WorkspacePanel` |
| CSS id        | `#workspace-panel`                                |
| Default width | 18 columns                                        |

Lists all registered workspaces. The selected workspace is prefixed with `> ` and highlighted. Workspaces with an active processing task show a yellow braille spinner badge (e.g., `my-app ⠹`). See [003-resource-management.md](003-resource-management.md) for CRUD operations.

**Internal widgets:**

| Widget ID         | Type       | Description                           |
| :---------------- | :--------- | :------------------------------------ |
| *(panel title)*   | `Label`    | "Workspaces", bold, accent background |
| `#workspace-list` | `ListView` | Scrollable list of workspace items    |

---

### ResizeHandle

| Property | Value                                         |
| :------- | :-------------------------------------------- |
| Class    | `humu.tui.widgets.resize_handle.ResizeHandle` |
| Width    | 1 column                                      |
| Glyph    | `⠿`                                           |

A 1-cell-wide draggable vertical separator. Three instances are placed between panels:

| Position                   | Target panel       | `invert` | Min width |
| :------------------------- | :----------------- | :------- | :-------- |
| Between Workspace and Room | `#workspace-panel` | `False`  | 10        |
| Between Room and Chat      | `#room-panel`      | `False`  | 8         |
| Between Chat and Agent     | `#agent-panel`     | `True`   | 10        |

The `invert=True` flag on the rightmost handle means dragging **left** makes the `AgentPanel` wider.

---

### RoomPanel

| Property      | Value                                   |
| :------------ | :-------------------------------------- |
| Class         | `humu.tui.widgets.room_panel.RoomPanel` |
| CSS id        | `#room-panel`                           |
| Default width | 14 columns                              |

Lists rooms in the currently selected workspace. Behavior mirrors `WorkspacePanel`: selected room prefixed `> `, spinner badge on processing rooms.

**Internal widgets:**

| Widget ID    | Type       | Description                      |
| :----------- | :--------- | :------------------------------- |
| *(title)*    | `Label`    | "Rooms", bold, accent background |
| `#room-list` | `ListView` | Scrollable list of room items    |

---

### ChatPanel

| Property | Value                                   |
| :------- | :-------------------------------------- |
| Class    | `humu.tui.widgets.chat_panel.ChatPanel` |
| Width    | `1fr` (fills remaining space)           |

The central panel. Title changes to "Chat - `<room>`" when a room is selected.

**Internal widgets (top to bottom):**

| Widget ID            | Type               | Description                                                     |
| :------------------- | :----------------- | :-------------------------------------------------------------- |
| *(panel title)*      | `Label`            | "Chat" or "Chat - `<room>`", bold, accent background            |
| `#chat-scroll`       | `VerticalScroll`   | Scrollable container for all chat messages                      |
| `#chat-messages`     | `Vertical`         | Direct parent of `ChatMessage` and `LoadingChatMessage` widgets |
| `#bottom-area`       | `Vertical`         | Docked to bottom; holds input box and autocomplete              |
| `#input-box`         | `Vertical`         | Contains the queue display and the chat input                   |
| `#queue-display`     | `Static`           | Shows pending message queue (hidden when empty)                 |
| `#chat-input`        | `ChatInput`        | Multi-line text input; submits on Enter, newline on Shift+Enter |
| `#path-autocomplete` | `PathAutocomplete` | 3-line dropdown for `@` path and `/` skill completions          |

#### ChatInput

Custom `TextArea` subclass (`humu.tui.widgets.chat_panel.ChatInput`):

- **Enter** — submits the message (clears the field).
- **Shift+Enter** — inserts a newline.
- **Up / Down** — navigate input history (when autocomplete is inactive).
- **Page Up / Page Down** — scroll the `#chat-scroll` area.
- `suppress_enter` flag — set to `True` while autocomplete is active so Enter selects a completion instead of submitting.

#### PathAutocomplete

Three-line dropdown that appears below the input when:
- `@` is typed — shows file/directory paths relative to the workspace root (exact prefix match then fuzzy subsequence search, max 15 results).
- `/` is typed at the start or after whitespace — shows built-in commands and installed plugin skills.

**`/` matching rules:**

| Input typed | Matches                                                                                                                |
| :---------- | :--------------------------------------------------------------------------------------------------------------------- |
| `/sk`       | Built-in commands starting with `sk` **+** any skill whose `skill-dir` part starts with `sk` (e.g. `my-mp:skill-name`) |
| `/my-mp:sk` | Skills whose full name starts with `my-mp:sk` (e.g. `my-mp:skill-name`)                                                |
| `/my-mp:`   | All skills in the `my-mp` marketplace                                                                                  |

When `partial` contains `:`, only full-name prefix matching is applied (built-in commands are excluded). When `partial` has no `:`, both the full name and the skill-dir part after `:` are checked.

Each autocomplete entry is displayed as `name  full description`. If an entry is wider than the widget, it is clipped and suffixed with `...` so each row always occupies exactly one line.

Built-in commands: `invite`, `kick`, `agents`, `rooms`, `status`, `compact`, `help`.

Navigate with **Up / Down**, confirm with **Enter** or **Tab**, dismiss with **Escape**.

---

### AgentPanel

| Property      | Value                                     |
| :------------ | :---------------------------------------- |
| Class         | `humu.tui.widgets.agent_panel.AgentPanel` |
| CSS id        | `#agent-panel`                            |
| Default width | 16 columns                                |

Lists agents in the currently selected room. The leader is prefixed with `*`; member agents with two spaces. Below each agent name, the configured model (e.g. `opus`, `sonnet`) is displayed in muted text.

**Double-click** on any item fires `AgentEditRequested`, which opens the agent edit dialog.

**Internal widgets:**

| Widget ID     | Type       | Description                                         |
| :------------ | :--------- | :-------------------------------------------------- |
| *(title)*     | `Label`    | "Agents", bold, accent background                   |
| `#agent-list` | `ListView` | Scrollable list of agent items (height: 2 per item) |

Each `ListItem` contains two `Label` widgets: the agent name and the model name (`.agent-model` class, `$text-muted` colour, left-padded).

---

## Chat Message Components

### ChatMessage

| Class | `humu.tui.widgets.chat_panel.ChatMessage` |
| :---- | :---------------------------------------- |

A vertical widget rendered for each message in `#chat-messages`. Contains:
- A sender label styled by role:
  - **Normal agent/user** — bold, accent colour.
  - **System message** — italic, muted colour.
  - **Error** — bold, error colour.
- When `context_pct` is provided (non-system, non-user agent messages), the label is rendered as `[agent-name] (42%)` to show how much of the agent's context window is consumed.
- The message text below the label, indented by 2 characters.

**Right-click** on any `ChatMessage` opens the `MessageDetailScreen` for that message.

---

### LoadingChatMessage

| Class | `humu.tui.widgets.chat_panel.LoadingChatMessage` |
| :---- | :----------------------------------------------- |

An animated placeholder appended to `#chat-messages` while an agent is processing. Shows a braille spinner (`⠋ ⠙ ⠹ …`) at 10 fps next to "thinking...". Removed when the agent's response arrives.

**Right-click** opens `MessageDetailScreen` with the live step log accumulated so far.

---

## Modal Screens

Modal screens are pushed on top of the main layout and block interaction with panels beneath.

### CreateWorkspaceScreen / CreateRoomScreen / CreateAgentScreen / ConfirmScreen

See [003-resource-management.md](003-resource-management.md) for creation, editing, and deletion workflows.

---

### MessageDetailScreen

| Class   | `humu.tui.screens.message_detail.MessageDetailScreen`    |
| :------ | :------------------------------------------------------- |
| Trigger | Right-click on any `ChatMessage` or `LoadingChatMessage` |
| Returns | `None`                                                   |

Scrollable process log for an agent's response. Renders each step in the order it occurred:

| Icon | Step type       | Content                                                                         |
| :--- | :-------------- | :------------------------------------------------------------------------------ |
| 💭    | `thinking`      | Thinking block text                                                             |
| 🔧    | `tool_use`      | Tool name + input (JSON / syntax-highlighted diff for `Edit` / bash for `Bash`) |
| ✓/✗  | `tool_result`   | Paired inline under its `tool_use` (or standalone if orphaned)                  |
| ⟳    | `task_progress` | Progress description + tool name                                                |

---

### PluginManagerScreen

| Class   | `humu.tui.screens.plugin_manager.PluginManagerScreen` |
| :------ | :---------------------------------------------------- |
| Trigger | `Ctrl+M`                                              |
| Returns | `None`                                                |

Full-screen modal (95% × 90%) with a two-pane layout:

**Left pane — Marketplaces (`#left-pane`)**

| Widget ID                 | Description                                                          |
| :------------------------ | :------------------------------------------------------------------- |
| `#marketplace-list`       | `ListView` of registered marketplaces; installed ones show a green ✓ |
| `#btn-add-marketplace`    | Opens `AddMarketplaceScreen`                                         |
| `#btn-remove-marketplace` | Removes the selected marketplace                                     |

**Right pane — Plugin Detail (`#right-pane`)**

| Widget ID        | Description                                                      |
| :--------------- | :--------------------------------------------------------------- |
| `#right-title`   | "Plugin Detail" or "Plugin — `<id>` (`<repo>`)"                  |
| `#plugin-scroll` | Scrollable list of skills with enable/disable toggles (`Switch`) |
| `#btn-install`   | Clone the marketplace repo                                       |
| `#btn-update`    | Pull latest changes                                              |
| `#btn-uninstall` | Remove the plugin directory                                      |
| `#status-bar`    | Single-line operation status message                             |

#### AddMarketplaceScreen

| Class | `humu.tui.screens.plugin_manager.AddMarketplaceScreen` |
| :---- | :----------------------------------------------------- |

Sub-modal for registering a new marketplace. Requires a GitHub repo (`owner/repo`). The ID is auto-derived from the repo name; if it conflicts with an existing marketplace, an additional ID field appears.

---

## Message & Event Bus

Key Textual `Message` types used across widgets:

| Message class         | Fired by         | Handled in  | Meaning                      |
| :-------------------- | :--------------- | :---------- | :--------------------------- |
| `WorkspaceSelected`   | `WorkspacePanel` | `HumuApp`   | User clicked a workspace     |
| `RoomSelected`        | `RoomPanel`      | `HumuApp`   | User clicked a room          |
| `MessageSubmitted`    | `ChatPanel`      | `HumuApp`   | User submitted a message     |
| `AgentEditRequested`  | `AgentPanel`     | `HumuApp`   | User double-clicked an agent |
| `ChatInput.Submitted` | `ChatInput`      | `ChatPanel` | Enter key in the chat input  |
