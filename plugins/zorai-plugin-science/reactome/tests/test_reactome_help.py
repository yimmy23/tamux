"""Offline tests for the reactome sub-plugin.

reactome hits the public Reactome Analysis + Content Services. We mock
http_client and verify a representative subcommand path. The script
exposes 53+ subcommands driven by a config dict; we test the most
useful (db-version) plus the config-driven parse_args path.
"""

from __future__ import annotations

from science_skills.scienceskillscommon import http_client


def test_reactome_help(invoke_cli) -> None:
    r = invoke_cli("reactome", "reactome_analysis.py", ["--help"])
    assert r.returncode == 0, r.stderr
    # Spot-check the named commands exposed in plugin.json.
    for sub in (
        "db-name", "db-version", "analyze", "token-result",
        "token-found-all", "token-not-found", "token-filter-species",
        "identifier", "search", "top-pathways",
    ):
        assert sub in r.stdout, f"reactome --help missing subcommand {sub!r}"


def test_reactome_commands_config_shape(load_subplugin_script) -> None:
    """The deepmind reactome script uses a config dict to drive all 53+
    subcommands. Verify the shape so future refactors of the deepmind
    script don't silently break our plugin.
    """
    mod = load_subplugin_script("reactome", "reactome_analysis.py")
    assert hasattr(mod, "COMMANDS")
    assert isinstance(mod.COMMANDS, list)
    assert len(mod.COMMANDS) >= 50, f"expected 50+ subcommands, got {len(mod.COMMANDS)}"
    # Every entry must have at least name + help.
    for cmd in mod.COMMANDS:
        assert "name" in cmd, f"COMMANDS entry missing 'name': {cmd}"
        assert "help" in cmd, f"COMMANDS entry missing 'help': {cmd}"
        assert "path" in cmd, f"COMMANDS entry missing 'path': {cmd}"


def test_reactome_db_version_offline(load_subplugin_script, monkeypatch, tmp_path, fake_http_response) -> None:
    """Mock the Reactome ContentService for /database/version and verify
    the script writes the canned response to disk.
    """
    mod = load_subplugin_script("reactome", "reactome_analysis.py")

    # db-version returns text/plain "89" (the current Reactome version).
    # Use the shared fake_http_response factory (real HttpResponse).
    resp = fake_http_response("89", content_encoding="identity")
    # Force content-type to text/plain so the script's text handler kicks in.
    resp.headers["content-type"] = "text/plain"
    monkeypatch.setattr(http_client.HttpClient, "fetch", lambda self, url, **kw: resp)

    out_path = tmp_path / "reactome_version.json"
    import argparse
    # Find the matching COMMANDS entry for db-version and build a minimal
    # namespace with the attrs the handler needs. (parse_args() is hard to
    # call in a test because it reads sys.argv; building a Namespace by
    # hand is simpler and isolates the test to the dispatch logic.)
    cfg = next(c for c in mod.COMMANDS if c["name"] == "db-version")
    ns = argparse.Namespace(
        _cfg=cfg,
        output=str(out_path),
    )
    mod._dispatch(ns)
    assert out_path.is_file()
    import json
    data = json.loads(out_path.read_text())
    # db-version wraps the text under wrap_key=database_version.
    assert data.get("database_version") == "89" or "89" in out_path.read_text()


def test_reactome_no_required_settings(plugin_manifests) -> None:
    """reactome has no required auth — settings should be empty or absent."""
    settings = plugin_manifests["reactome"].get("settings", {})
    assert not any(s.get("required") is True for s in settings.values()), (
        f"reactome requires no auth; settings must not have required=True: {settings!r}"
    )


def test_reactome_skill_md_lists_named_commands(plugin_manifests) -> None:
    """The skill.md must reference all 11 named commands so the agent
    knows what's available without going through the catch-all.
    """
    from pathlib import Path
    manifest = plugin_manifests["reactome"]
    cmds = set(manifest["commands"].keys())
    skill_path = Path(__file__).resolve().parents[1] / manifest["skills"][0]
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"reactome skill.md does not mention command {cmd!r}"
