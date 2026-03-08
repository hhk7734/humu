from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, ListItem, ListView, Static, Switch

from humu.services.storage import Storage


class AddMarketplaceScreen(ModalScreen[tuple[str, str] | None]):
    """Dialog for adding a new marketplace.

    Only the GitHub repo (owner/repo) is required.
    The ID is auto-derived from the repo name (yyy of xxx/yyy).
    The ID field is shown only when there is a conflict with an existing marketplace.
    """

    BINDINGS = [("escape", "dismiss", "Cancel")]

    DEFAULT_CSS = """
    AddMarketplaceScreen {
        align: center middle;
        background: $background 60%;
    }
    AddMarketplaceScreen #dialog {
        width: 60;
        height: auto;
        border: solid $accent;
        background: $surface;
        padding: 1 2;
    }
    AddMarketplaceScreen #title {
        text-style: bold;
        margin-bottom: 1;
    }
    AddMarketplaceScreen Label {
        margin-top: 1;
    }
    AddMarketplaceScreen Input {
        margin-bottom: 0;
    }
    AddMarketplaceScreen #id-section {
        display: none;
        height: auto;
    }
    AddMarketplaceScreen #id-section.visible {
        display: block;
    }
    AddMarketplaceScreen #actions {
        layout: horizontal;
        margin-top: 1;
        height: auto;
    }
    AddMarketplaceScreen Button {
        margin-right: 1;
    }
    """

    def __init__(self, existing_ids: set[str]) -> None:
        super().__init__()
        self._existing_ids = existing_ids

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog"):
            yield Label("Add Marketplace", id="title")
            yield Label("GitHub repo (e.g. hhk7734/hhk7734)")
            yield Input(placeholder="owner/repo", id="input-repo")
            with Vertical(id="id-section"):
                yield Label(
                    "ID conflicts with an existing marketplace. Enter a different ID:"
                )
                yield Input(placeholder="marketplace-id", id="input-id")
            with Horizontal(id="actions"):
                yield Button("Add", variant="primary", id="btn-add")
                yield Button("Cancel", id="btn-cancel")

    def on_input_changed(self, event: Input.Changed) -> None:
        if event.input.id != "input-repo":
            return
        repo = event.value.strip()
        auto_id = repo.split("/")[-1] if "/" in repo else repo
        id_section = self.query_one("#id-section")
        if auto_id and auto_id in self._existing_ids:
            id_section.add_class("visible")
            id_input = self.query_one("#input-id", Input)
            if not id_input.value:
                id_input.value = auto_id
        else:
            id_section.remove_class("visible")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        if event.button.id == "btn-add":
            repo = self.query_one("#input-repo", Input).value.strip()
            if not repo:
                self.notify("GitHub repo is required.", severity="warning")
                return
            auto_id = repo.split("/")[-1] if "/" in repo else repo
            id_section = self.query_one("#id-section")
            if "visible" in id_section.classes:
                mid = self.query_one("#input-id", Input).value.strip()
                if not mid:
                    self.notify(
                        "ID is required to resolve the conflict.", severity="warning"
                    )
                    return
            else:
                mid = auto_id
            self.dismiss((mid, repo))
        else:
            self.dismiss(None)

    def on_input_submitted(self, event: Input.Submitted) -> None:
        if event.input.id == "input-repo":
            id_section = self.query_one("#id-section")
            if "visible" in id_section.classes:
                self.query_one("#input-id", Input).focus()
            else:
                self.query_one("#btn-add", Button).press()
        else:
            self.query_one("#btn-add", Button).press()


class PluginManagerScreen(ModalScreen[None]):
    """Browse marketplaces and manage installed plugins."""

    BINDINGS = [("escape", "dismiss", "Close")]

    DEFAULT_CSS = """
    PluginManagerScreen {
        align: center middle;
        background: $background 60%;
    }
    PluginManagerScreen #dialog {
        width: 95%;
        height: 90%;
        border: solid $accent;
        background: $surface;
        layout: vertical;
    }
    PluginManagerScreen #title {
        text-style: bold;
        padding: 0 1;
        background: $accent;
        color: $text;
        width: 1fr;
    }
    PluginManagerScreen #body {
        height: 1fr;
        layout: horizontal;
    }
    /* Left pane: marketplace list */
    PluginManagerScreen #left-pane {
        width: 30;
        border-right: solid $panel;
        layout: vertical;
    }
    PluginManagerScreen #left-pane .pane-title {
        padding: 0 1;
        background: $panel;
        text-style: bold;
        height: 1;
    }
    PluginManagerScreen #marketplace-list {
        height: 1fr;
    }
    PluginManagerScreen #left-actions {
        height: auto;
        layout: horizontal;
        padding: 0 1;
    }
    PluginManagerScreen #left-actions Button {
        width: 1fr;
        margin: 0 0 0 0;
        border: none;
    }
    /* Right pane: plugin detail */
    PluginManagerScreen #right-pane {
        width: 1fr;
        layout: vertical;
    }
    PluginManagerScreen #right-pane .pane-title {
        padding: 0 1;
        background: $panel;
        text-style: bold;
        height: 1;
    }
    PluginManagerScreen #plugin-scroll {
        height: 1fr;
        padding: 1 2;
    }
    PluginManagerScreen #plugin-actions {
        height: auto;
        layout: horizontal;
        padding: 0 1;
    }
    PluginManagerScreen #plugin-actions Button {
        margin-right: 1;
    }
    PluginManagerScreen .skill-item {
        padding: 0 0 1 0;
        height: auto;
    }
    PluginManagerScreen .skill-header {
        layout: horizontal;
        height: 1;
        align: left middle;
    }
    PluginManagerScreen .skill-switch {
        width: auto;
        height: 1;
        border: none;
        background: transparent;
        margin: 0;
        padding: 0;
    }
    PluginManagerScreen .skill-switch:focus {
        border: none;
        height: 1;
    }
    PluginManagerScreen .skill-name {
        color: $accent;
        text-style: bold;
        width: 1fr;
        padding: 0 0 0 1;
        height: 1;
        content-align: left middle;
    }
    PluginManagerScreen .skill-desc {
        color: $text-muted;
        padding: 0 0 0 6;
    }
    PluginManagerScreen #status-bar {
        height: 1;
        padding: 0 1;
        background: $panel;
        color: $text-muted;
    }
    PluginManagerScreen ListView > ListItem.--highlight {
        background: $accent 30%;
    }
    PluginManagerScreen .marketplace-item {
        padding: 0 1;
        height: auto;
    }
    PluginManagerScreen .installed-badge {
        color: $success;
    }
    """

    def __init__(self, storage: Storage) -> None:
        super().__init__()
        self._storage = storage
        self._selected: dict | None = None  # currently selected marketplace entry

    def compose(self) -> ComposeResult:
        with Vertical(id="dialog"):
            yield Label("Plugin Manager", id="title")
            with Horizontal(id="body"):
                with Vertical(id="left-pane"):
                    yield Label("Marketplaces", classes="pane-title")
                    yield ListView(id="marketplace-list")
                    with Horizontal(id="left-actions"):
                        yield Button(
                            "+ Add", id="btn-add-marketplace", variant="primary"
                        )
                        yield Button(
                            "- Remove", id="btn-remove-marketplace", variant="error"
                        )
                with Vertical(id="right-pane"):
                    yield Label("Plugin Detail", classes="pane-title", id="right-title")
                    with VerticalScroll(id="plugin-scroll"):
                        yield Static(
                            "Select a marketplace on the left.", id="placeholder"
                        )
                    with Horizontal(id="plugin-actions"):
                        yield Button("Install", id="btn-install", variant="primary")
                        yield Button("Update", id="btn-update")
                        yield Button("Uninstall", id="btn-uninstall", variant="error")
            yield Static("", id="status-bar")

    def on_mount(self) -> None:
        self._refresh_marketplace_list()
        self._update_plugin_buttons()

    def _refresh_marketplace_list(self) -> None:
        lv = self.query_one("#marketplace-list", ListView)
        lv.clear()
        for m in self._storage.list_marketplaces():
            installed = self._storage.is_plugin_installed(m["id"])
            badge = " [green]✓[/green]" if installed else ""
            lv.append(
                ListItem(
                    Static(
                        f"{m['id']}{badge}\n[dim]{m['repo']}[/dim]",
                        classes="marketplace-item",
                    ),
                    name=m["id"],
                )
            )

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        marketplace_id = event.item.name
        marketplaces = {m["id"]: m for m in self._storage.list_marketplaces()}
        self._selected = marketplaces.get(marketplace_id)
        if self._selected:
            self._show_plugin_detail(self._selected)
        self._update_plugin_buttons()

    def _show_plugin_detail(self, marketplace: dict) -> None:
        mid = marketplace["id"]
        repo = marketplace["repo"]
        installed = self._storage.is_plugin_installed(mid)

        self.query_one("#right-title", Label).update(
            f"Plugin — {mid}  [dim]({repo})[/dim]"
        )

        scroll = self.query_one("#plugin-scroll", VerticalScroll)
        scroll.remove_children()

        if not installed:
            scroll.mount(
                Static(
                    f"Not installed.\nPress [bold]Install[/bold] to clone from github.com/{repo}"
                )
            )
            return

        # Show skills
        plugin_dir = self._storage.plugin_dir(mid)
        skills_dir = plugin_dir / "skills"
        if not skills_dir.exists():
            scroll.mount(Static("Installed, but no skills/ directory found."))
            return

        skill_dirs = sorted(d for d in skills_dir.iterdir() if d.is_dir())
        if not skill_dirs:
            scroll.mount(Static("Installed, but no skills found."))
            return

        for skill_dir in skill_dirs:
            skill_md = skill_dir / "SKILL.md"
            _, description = "", ""
            if skill_md.exists():
                _, description = self._storage._parse_skill_frontmatter(
                    skill_md.read_text()
                )
            full_name = f"{mid}:{skill_dir.name}"
            enabled = self._storage.is_skill_enabled(full_name)
            # CSS IDs cannot contain ":" — encode it as "_COLON_"
            toggle_id = f"skill-toggle-{full_name.replace(':', '_COLON_')}"
            # Build widgets separately; mount parent first, then children.
            block = Vertical(classes="skill-item")
            scroll.mount(block)
            header = Horizontal(classes="skill-header")
            block.mount(header)
            header.mount(
                Switch(value=enabled, id=toggle_id, classes="skill-switch"),
                Static(f"/{full_name}", classes="skill-name"),
            )
            if description:
                block.mount(Static(description, classes="skill-desc"))

    def _update_plugin_buttons(self) -> None:
        installed = self._selected and self._storage.is_plugin_installed(
            self._selected["id"]
        )
        self.query_one("#btn-install", Button).disabled = not self._selected or bool(
            installed
        )
        self.query_one("#btn-update", Button).disabled = not installed
        self.query_one("#btn-uninstall", Button).disabled = not installed

    def on_button_pressed(self, event: Button.Pressed) -> None:
        btn_id = event.button.id

        if btn_id == "btn-add-marketplace":
            existing_ids = {m["id"] for m in self._storage.list_marketplaces()}
            self.app.push_screen(
                AddMarketplaceScreen(existing_ids), self._on_marketplace_added
            )

        elif btn_id == "btn-remove-marketplace":
            if self._selected:
                self._storage.remove_marketplace(self._selected["id"])
                self._selected = None
                self._refresh_marketplace_list()
                self._update_plugin_buttons()
                self.query_one("#right-title", Label).update("Plugin Detail")
                scroll = self.query_one("#plugin-scroll", VerticalScroll)
                scroll.remove_children()
                scroll.mount(
                    Static("Select a marketplace on the left.", id="placeholder")
                )
                self._set_status(f"Marketplace removed.")

        elif btn_id == "btn-install":
            if self._selected:
                self._set_status(f"Installing {self._selected['repo']} ...")
                ok, msg = self._storage.install_plugin(
                    self._selected["id"], self._selected["repo"]
                )
                self._set_status(
                    f"{'[green]' if ok else '[red]'}{msg}{'[/green]' if ok else '[/red]'}"
                )
                if ok:
                    self._refresh_marketplace_list()
                    self._show_plugin_detail(self._selected)
                    self._update_plugin_buttons()

        elif btn_id == "btn-update":
            if self._selected:
                self._set_status(f"Updating {self._selected['id']} ...")
                ok, msg = self._storage.update_plugin(self._selected["id"])
                self._set_status(
                    f"{'[green]' if ok else '[red]'}{msg}{'[/green]' if ok else '[/red]'}"
                )
                if ok:
                    self._show_plugin_detail(self._selected)

        elif btn_id == "btn-uninstall":
            if self._selected:
                ok, msg = self._storage.uninstall_plugin(self._selected["id"])
                self._set_status(
                    f"{'[green]' if ok else '[red]'}{msg}{'[/green]' if ok else '[/red]'}"
                )
                if ok:
                    self._refresh_marketplace_list()
                    self._show_plugin_detail(self._selected)
                    self._update_plugin_buttons()

    def _on_marketplace_added(self, result: tuple[str, str] | None) -> None:
        if result:
            mid, repo = result
            self._storage.add_marketplace(mid, repo)
            self._refresh_marketplace_list()
            self._set_status(f"Installing {repo} ...")
            ok, msg = self._storage.install_plugin(mid, repo)
            self._set_status(
                f"{'[green]' if ok else '[red]'}{msg}{'[/green]' if ok else '[/red]'}"
            )
            if ok:
                self._refresh_marketplace_list()
                # Select the newly added marketplace
                lv = self.query_one("#marketplace-list", ListView)
                for item in lv.query(ListItem):
                    if item.name == mid:
                        lv.index = list(lv.query(ListItem)).index(item)
                        break
                marketplaces = {m["id"]: m for m in self._storage.list_marketplaces()}
                self._selected = marketplaces.get(mid)
                if self._selected:
                    self._show_plugin_detail(self._selected)
                self._update_plugin_buttons()

    def on_switch_changed(self, event: Switch.Changed) -> None:
        event.stop()
        cid = event.switch.id or ""
        if not cid.startswith("skill-toggle-"):
            return
        full_name = cid[len("skill-toggle-") :].replace("_COLON_", ":")
        if event.value:
            self._storage.enable_skill(full_name)
            self._set_status(f"Skill '/{full_name}' enabled.")
        else:
            self._storage.disable_skill(full_name)
            self._set_status(f"Skill '/{full_name}' disabled.")

    def _set_status(self, msg: str) -> None:
        self.query_one("#status-bar", Static).update(msg)
