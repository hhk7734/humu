import pytest
from pathlib import Path

from humu.plugins.manager import PluginManager


@pytest.fixture
def plugin_dir(tmp_path):
    # Create a fake marketplace structure
    mp_dir = tmp_path / "marketplaces" / "owner" / "repo"
    mp_dir.mkdir(parents=True)
    (mp_dir / ".claude-plugin").mkdir()
    (mp_dir / ".claude-plugin" / "marketplace.json").write_text(
        '{"plugins": ["my-plugin"]}'
    )
    plugin_path = mp_dir / "plugins" / "my-plugin"
    plugin_path.mkdir(parents=True)
    (plugin_path / "plugin.yaml").write_text("name: my-plugin\ndescription: Test plugin")
    skill_dir = plugin_path / "skills" / "my-skill"
    skill_dir.mkdir(parents=True)
    (skill_dir / "SKILL.md").write_text(
        "---\nname: my-skill\ndescription: A test skill\n---\n\nSkill body here."
    )

    plugins_dir = tmp_path / "plugins"
    plugins_dir.mkdir()
    return tmp_path


def test_list_marketplace_plugins(plugin_dir):
    manager = PluginManager(
        marketplaces_dir=plugin_dir / "marketplaces",
        plugins_dir=plugin_dir / "plugins",
    )
    plugins = manager.list_marketplace_plugins("owner/repo")
    assert "my-plugin" in plugins


def test_install_plugin(plugin_dir):
    manager = PluginManager(
        marketplaces_dir=plugin_dir / "marketplaces",
        plugins_dir=plugin_dir / "plugins",
    )
    manager.install_from_marketplace("owner/repo", "my-plugin")
    assert (plugin_dir / "plugins" / "my-plugin" / "plugin.yaml").exists()


def test_list_skills(plugin_dir):
    manager = PluginManager(
        marketplaces_dir=plugin_dir / "marketplaces",
        plugins_dir=plugin_dir / "plugins",
    )
    manager.install_from_marketplace("owner/repo", "my-plugin")
    skills = manager.list_skills()
    assert len(skills) == 1
    assert skills[0]["name"] == "my-plugin:my-skill"


def test_get_skill_content(plugin_dir):
    manager = PluginManager(
        marketplaces_dir=plugin_dir / "marketplaces",
        plugins_dir=plugin_dir / "plugins",
    )
    manager.install_from_marketplace("owner/repo", "my-plugin")
    content = manager.get_skill_content("my-plugin:my-skill")
    assert content is not None
    assert "Skill body here." in content
