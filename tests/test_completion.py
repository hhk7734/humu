import os

import pytest

from humu.client.completion import fuzzy_match, list_paths, render_dropdown


def test_fuzzy_match_basic():
    assert fuzzy_match("abc", "aXbXc") is True
    assert fuzzy_match("abc", "ABC") is True  # case-insensitive
    assert fuzzy_match("abc", "axc") is False
    assert fuzzy_match("", "anything") is True
    assert fuzzy_match("a", "") is False


def test_list_paths_prefix(tmp_path):
    (tmp_path / "src").mkdir()
    (tmp_path / "setup.py").touch()
    (tmp_path / "README.md").touch()

    results = list_paths(str(tmp_path), "s")
    names = [os.path.basename(r.rstrip("/")) for r in results]
    assert "src" in names
    assert "setup.py" in names
    assert "README.md" not in names


def test_list_paths_empty_partial(tmp_path):
    (tmp_path / "a").mkdir()
    (tmp_path / "b").touch()

    results = list_paths(str(tmp_path), "")
    assert len(results) >= 2


def test_list_paths_subsequence(tmp_path):
    (tmp_path / "project").mkdir()
    (tmp_path / "project" / "config.py").touch()

    results = list_paths(str(tmp_path), "pcfg")
    # Should find project/config.py via subsequence BFS
    matches = [r for r in results if "config" in r]
    assert len(matches) >= 1


def test_list_paths_max_results(tmp_path):
    for i in range(20):
        (tmp_path / f"file_{i:02d}.txt").touch()

    results = list_paths(str(tmp_path), "file", max_results=5)
    assert len(results) <= 5


def test_render_dropdown():
    items = ["alpha", "beta", "gamma", "delta", "epsilon"]
    text = render_dropdown(items, selected_index=1, width=40, num_lines=5)
    rendered = str(text)
    assert "beta" in rendered
    assert "alpha" in rendered


def test_render_dropdown_empty():
    text = render_dropdown([], selected_index=0, width=40, num_lines=5)
    rendered = str(text)
    # Should produce empty lines
    assert rendered.strip() == ""
