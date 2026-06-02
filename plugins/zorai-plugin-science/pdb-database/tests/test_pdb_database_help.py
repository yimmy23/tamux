"""Offline tests for the pdb-database sub-plugin.

pdb hits the public RCSB Data API + Search API + file download endpoints
using 4 separate scripts (download_coordinate_files, fetch_pdb_metadata,
fetch_schema, search_pdb). We exercise the --help surface and verify
the plugin's command surface + settings.
"""

from __future__ import annotations

from pathlib import Path


def test_pdb_all_four_help(invoke_cli) -> None:
    for s in ("download_coordinate_files.py", "fetch_pdb_metadata.py", "fetch_schema.py", "search_pdb.py"):
        r = invoke_cli("pdb-database", s, ["--help"])
        assert r.returncode == 0, f"{s} --help failed: {r.stderr}"


def test_pdb_skill_md_lists_all_four_commands(plugin_manifests) -> None:
    """The skill.md must reference all 4 plugin.json commands by name."""
    manifest = plugin_manifests["pdb-database"]
    cmds = set(manifest["commands"].keys())
    skill_path = Path(__file__).resolve().parents[1] / manifest["skills"][0]
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"pdb-database skill.md does not mention command {cmd!r}"


def test_pdb_no_required_settings(plugin_manifests) -> None:
    """pdb-database has no required auth — settings should be empty or absent."""
    settings = plugin_manifests["pdb-database"].get("settings", {})
    assert not any(s.get("required") is True for s in settings.values()), (
        f"pdb-database requires no auth; settings must not have required=True: {settings!r}"
    )


def test_pdb_commands_reference_real_scripts(plugin_dir: Path) -> None:
    """Every command must point to a real .py in pdb-database/scripts/."""
    import json
    manifest = json.load(open(plugin_dir / "pdb-database" / "plugin.json"))
    scripts_dir = plugin_dir / "pdb-database" / "scripts"
    assert scripts_dir.is_dir(), f"missing {scripts_dir}"
    py_files = {p.name for p in scripts_dir.glob("*.py")}
    assert len(py_files) >= 4, f"expected 4 pdb scripts, got {py_files}"
    for name, cdef in manifest["commands"].items():
        py = cdef["python"]
        script = py["command"].split()[1][len("scripts/"):]
        assert script in py_files, f"{name} references missing script {script}"


def test_pdb_promoted_from_stub(plugin_manifests) -> None:
    """The pdb sub-plugin's plugin.json has multiple named commands, proving it was
    promoted from the long-tail stub (which had only a single `run` command).
    """
    manifest = plugin_manifests["pdb-database"]
    assert len(manifest["commands"]) > 1, (
        f"pdb-database still looks like a stub: only {len(manifest['commands'])} commands"
    )
    assert "run" not in manifest["commands"], (
        "pdb-database is a full sub-plugin; the generic `run` catch-all should not be present"
    )
