"""Offline tests for the ncbi-sequence-fetch sub-plugin.

ncbi hits the public NCBI E-utilities. The script has 10 subcommands
exposed; we promote the 7 most-useful to named commands and keep a `run`
catch-all for the other 3. We verify the help surface and the plugin
schema.
"""

from __future__ import annotations

from science_skills.scienceskillscommon import http_client


def test_ncbi_help(invoke_cli) -> None:
    r = invoke_cli("ncbi-sequence-fetch", "ncbi_fetch.py", ["--help"])
    assert r.returncode == 0, r.stderr
    # Spot-check all 7 named subcommands.
    for sub in (
        "fetch-protein", "fetch-nucleotide", "cds-translate", "search",
        "elink", "gene-protein", "locus-protein",
    ):
        assert sub in r.stdout, f"ncbi --help missing subcommand {sub!r}"


def test_ncbi_skill_md_lists_named_commands(plugin_manifests) -> None:
    from pathlib import Path
    manifest = plugin_manifests["ncbi-sequence-fetch"]
    cmds = set(manifest["commands"].keys())
    skill_path = Path(__file__).resolve().parents[1] / manifest["skills"][0]
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"ncbi-sequence-fetch skill.md does not mention command {cmd!r}"


def test_ncbi_optional_api_key_setting(plugin_manifests) -> None:
    """NCBI_API_KEY is optional (raises rate limit, not required)."""
    settings = plugin_manifests["ncbi-sequence-fetch"].get("settings", {})
    assert "NCBI_API_KEY" in settings
    s = settings["NCBI_API_KEY"]
    assert s["required"] is False
    assert s["secret"] is True


def test_ncbi_promoted_from_stub(plugin_manifests) -> None:
    """Promoted: 7 named commands + 1 `run` catch-all. The stub had only `run`."""
    manifest = plugin_manifests["ncbi-sequence-fetch"]
    assert "run" in manifest["commands"], "ncbi run catch-all missing"
    assert len(manifest["commands"]) >= 7, (
        f"expected 7+ named commands, got {len(manifest['commands'])}"
    )


def test_ncbi_settings_module_imports(load_subplugin_script) -> None:
    """Smoke check: the script module imports cleanly with the vendored
    scienceskillscommon shim (real loading exercise, not mock)."""
    mod = load_subplugin_script("ncbi-sequence-fetch", "ncbi_fetch.py")
    # The script exposes the 10 cmd_* handlers.
    handlers = [n for n in dir(mod) if n.startswith("cmd_")]
    assert len(handlers) >= 8, f"expected 8+ cmd_* handlers, got {len(handlers)}: {handlers}"
    # And the http_client it uses is namespaced under scienceskillscommon.
    assert http_client is mod.http_client
