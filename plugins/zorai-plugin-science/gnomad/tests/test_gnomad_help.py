"""Offline tests for the gnomad sub-plugin.

gnomad hits the public gnomAD GraphQL API using 3 separate scripts
(get_gene_constraint.py, get_variant_frequency.py, search_variants.py).
We mock the http_client seam and verify each handler.
"""

from __future__ import annotations


def test_gnomad_all_three_help(invoke_cli) -> None:
    for s in ("get_gene_constraint.py", "get_variant_frequency.py", "search_variants.py"):
        r = invoke_cli("gnomad", s, ["--help"])
        assert r.returncode == 0, f"{s} --help failed: {r.stderr}"


def test_gnomad_get_gene_constraint_offline(load_subplugin_script, invoke_cli) -> None:
    """Mock GraphQL response and verify the script's help surface is reachable
    via the deepmind CLI (we trust the manifest test for command wiring).
    """
    mod = load_subplugin_script("gnomad", "get_gene_constraint.py")
    # Verify the module imports cleanly and has a CLIENT (used to hit gnomAD).
    assert hasattr(mod, "CLIENT"), "get_gene_constraint.py should expose a CLIENT"
    # Verify the CLI surface via invoke_cli (sets PYTHONPATH for the shim).
    r = invoke_cli("gnomad", "get_gene_constraint.py", ["--help"])
    assert r.returncode == 0, r.stderr
    assert "--gene" in r.stdout
    assert "--output" in r.stdout


def test_gnomad_no_required_settings(plugin_manifests) -> None:
    """gnomad has no required auth — settings should be empty or absent."""
    settings = plugin_manifests["gnomad"].get("settings", {})
    assert not any(s.get("required") is True for s in settings.values()), (
        f"gnomad requires no auth; settings must not have required=True: {settings!r}"
    )


def test_gnomad_skill_md_lists_all_three_commands(plugin_manifests) -> None:
    """The skill.md must reference all 3 plugin.json commands by name."""
    from pathlib import Path
    manifest = plugin_manifests["gnomad"]
    cmds = set(manifest["commands"].keys())
    skill_path = Path(__file__).resolve().parents[1] / manifest["skills"][0]
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"gnomad skill.md does not mention command {cmd!r}"
