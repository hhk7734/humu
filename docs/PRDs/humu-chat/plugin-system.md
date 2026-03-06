# Humu — Plugin & Marketplace System

## Overview

The plugin system lets users extend Humu with **skills** sourced from external GitHub repositories (marketplaces). A skill is a structured Markdown file (`SKILL.md`) that carries a name, a description, and a body of instructions injected into agent system prompts at runtime.

## Concepts

### Marketplace

A GitHub repository that follows the Humu plugin directory layout. A marketplace is identified by:

| Field  | Description                              |
| :----- | :--------------------------------------- |
| `id`   | Short identifier (defaults to repo name) |
| `repo` | GitHub repo in `owner/repo` format       |

Marketplaces are stored in `~/.humu/marketplaces.json`.

### Plugin

When a marketplace is installed, its repository is shallow-cloned into `~/.humu/plugins/<marketplace_id>/`. The plugin may expose one or more skills inside a `skills/` directory.

```
~/.humu/plugins/
+-- <marketplace_id>/
    +-- skills/
        +-- <skill-name>/
            +-- SKILL.md
```

### Skill

Each `SKILL.md` file follows this format:

```markdown
---
name: (optional display label — not used as identifier)
description: One-line description shown to leader agents for routing decisions.
---

# Skill body

Any markdown content. This is injected verbatim into agent system prompts when
the skill is active.
```

Skills are discovered by scanning `~/.humu/plugins/*/skills/*/SKILL.md`.

#### Skill Naming

A skill's **registered name** is `{marketplace_id}:{skill_dir_name}` — derived from the directory structure, not the frontmatter `name` field. The frontmatter `name` field is ignored at runtime; only `description` and the body content are used.

```
~/.humu/plugins/my-mp/skills/code-review/SKILL.md
                 ↑          ↑
          marketplace_id  skill_dir_name
          → full name: "my-mp:code-review"
```

#### Enable / Disable

Individual skills can be **enabled** or **disabled** without uninstalling the plugin. The `disabled` list in `~/.humu/skills_config.json` stores full `marketplace:skill` names.

## Plugin Manager UI

Opened with `Ctrl+M`. A full-screen modal with two panes:

```
+-- Marketplaces ------+-- Plugin Detail ---------------------------+
|                      |                                            |
| my-mp  [✓]           | Plugin — my-mp  (owner/my-mp)             |
| another-mp           |                                            |
|                      |   [ON]  /my-mp:code-review                |
|                      |         Review PRs for bugs and style.    |
|                      |                                            |
|                      |   [OFF] /my-mp:brainstorm                 |
|                      |         Explore ideas before coding.      |
|                      |                                            |
+ Add  |  Remove ------+ Install | Update | Uninstall -------------+
```

### Left Pane — Marketplaces

- Lists all registered marketplaces.
- Installed marketplaces show a green checkmark.
- **Add** opens a dialog asking for the GitHub repo (`owner/repo`). The ID is auto-derived from the repo name; if it conflicts, a custom ID field appears.
- **Remove** unregisters the marketplace (does not uninstall files).

### Right Pane — Plugin Detail

- Shows skills found in the installed plugin with enable/disable toggles.
- **Install** — clones the repo from GitHub (`git clone --depth=1`).
- **Update** — pulls latest changes (`git pull --ff-only`).
- **Uninstall** — removes the plugin directory.

## Skill Invocation

Users invoke a skill by prefixing their message with `/<marketplace>:<skill>`:

```
/my-mp:code-review What should I look for in this PR?
```

The router:

1. Strips the `/<marketplace>:<skill>` prefix.
2. Loads the skill body from `~/.humu/plugins/<marketplace>/skills/<skill>/SKILL.md`.
3. Injects the skill body into the leader and relevant member agent prompts as:
   ```
   ## Active Skill: <marketplace>:<skill>
   <skill body>
   ```

If the skill is not found (wrong name or not installed), a system error message is shown and no agents are queried.

The `/` autocomplete in `ChatInput` shows all available skills in `marketplace:skill  description` format alongside built-in commands.

## Skill Context Injection

Skills influence agent behavior at two levels:

| Level         | Content injected                                          | Timing                                      |
| :------------ | :-------------------------------------------------------- | :------------------------------------------ |
| Session-level | `## Available Skills` with all `marketplace:skill` names  | New session only (first query)              |
| Message-level | `## Active Skill: <marketplace>:<skill>` with full body   | Every message that uses `/<marketplace>:<skill>` |

The **session-level** injection happens once so the leader knows what skills exist for routing decisions. The **message-level** injection happens on every invocation to provide the detailed instructions.

## Storage

| Path                                     | Contents                                     |
| :--------------------------------------- | :------------------------------------------- |
| `~/.humu/marketplaces.json`              | `[{"id": "...", "repo": "owner/repo"}, ...]` |
| `~/.humu/plugins/<id>/`                  | Cloned marketplace repository                |
| `~/.humu/plugins/<id>/skills/*/SKILL.md` | Skill definitions                            |
| `~/.humu/skills_config.json`             | `{"disabled": ["mp:skill-a", "mp:skill-b"]}` |

## Error Handling

| Scenario                            | Behavior                                                             |
| :---------------------------------- | :------------------------------------------------------------------- |
| `/<marketplace>:<skill>` not found  | System error in chat; agents not queried                             |
| `git clone` fails                   | Status bar shows error message; marketplace stays registered         |
| `git pull` fails                    | Status bar shows error; installed files unchanged                    |
| `SKILL.md` missing or malformed     | Skill silently skipped during discovery                              |
| Disabled skill invoked with `/mp:name` | Skill body is still loaded (disable only hides from session context) |

## Non-Goals

- No version pinning (always uses latest default branch)
- No signature verification of plugin content
- No sandboxing of skill instructions
