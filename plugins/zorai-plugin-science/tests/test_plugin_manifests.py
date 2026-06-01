"""Offline tests for zorai-plugin-science plugin manifests.

Cross-cutting assertions that apply to every sub-plugin's plugin.json
and to the package.json root. These tests do not run scripts — they
inspect the manifest files directly and the `skills/<id>.md` mirror
convention.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

# Sub-plugins declared in conftest.py's SUBPLUGIN_SCRIPTS map.
SUBPLUGINS = [
    # Full sub-plugins (hand-written plugin.json with named commands).
    "alphagenome",
    "alphafold-database",
    "uniprot",
    "clinvar",
    "chembl",
    "openalex",
    "ensembl",
    "reactome",
    "gnomad",
    # Long-tail sub-plugins (stub plugin.json with a single `run` command).
    "clinical-trials-database",
    "dbsnp-database",
    "embl-ebi-ols",
    "encode-ccres-database",
    "foldseek-structural-search",
    "gtex-database",
    "human-protein-atlas-database",
    "interpro-database",
    "jaspar-database",
    "literature-search-arxiv",
    "literature-search-biorxiv",
    "literature-search-europepmc",
    "ncbi-sequence-fetch",
    "openfda-database",
    "opentargets-database",
    "pdb-database",
    "protein-sequence-msa",
    "protein-sequence-similarity-search",
    "pubchem-database",
    "pubmed-database",
    "pymol",  # binary, no Python script; stub has empty commands
    "quickgo-database",
    "string-database",
    "ucsc-conservation-and-tfbs",
    "unibind-database",
]


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_plugin_json_loads(sub: str, plugin_dir: Path) -> None:
    """Each sub-plugin's plugin.json must parse as valid JSON."""
    path = plugin_dir / sub / "plugin.json"
    assert path.is_file(), f"missing {path}"
    with path.open() as f:
        manifest = json.load(f)
    assert isinstance(manifest, dict)


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_plugin_json_required_fields(sub: str, plugin_manifests: dict) -> None:
    manifest = plugin_manifests[sub]
    for key in ("name", "version", "schema_version", "description", "license"):
        assert key in manifest, f"{sub} missing required field {key!r}"
    assert manifest["name"] == sub, f"{sub} plugin.json name mismatch: {manifest['name']!r}"
    assert manifest["schema_version"] == 1


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_python_block_is_pep723(sub: str, plugin_manifests: dict) -> None:
    """The plugin.json python block must use env:false and an empty deps list
    so that the deepmind scripts' PEP 723 inline deps drive the resolution.
    """
    py = plugin_manifests[sub].get("python")
    assert py is not None, f"{sub} missing python block"
    assert py.get("env") is False, f"{sub} python.env must be false (PEP 723)"
    assert py.get("dependencies") == [], f"{sub} python.dependencies must be [] (PEP 723)"


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_commands_reference_real_scripts(sub: str, plugin_manifests: dict, plugin_dir: Path) -> None:
    """Every commands.<name>.python.command must:
      - start with 'uv run scripts/'
      - reference a script that actually exists in <sub>/scripts/
    Skips stubs with empty `commands` (pymol — binary, no Python entry).
    """
    manifest = plugin_manifests[sub]
    cmds = manifest.get("commands", {})
    if not cmds:
        # Documentation-only stub (pymol). Just verify skills[] is present.
        assert manifest.get("skills"), f"{sub} has no commands and no skills"
        return
    scripts_dir = plugin_dir / sub / "scripts"
    for name, cdef in cmds.items():
        py = cdef.get("python")
        assert py is not None, f"{sub}.{name} missing python block"
        cmd = py["command"]
        assert cmd.startswith("uv run scripts/"), (
            f"{sub}.{name} command must start with 'uv run scripts/'; got: {cmd!r}"
        )
        # Extract the script filename: "uv run scripts/<name>.py ..."
        first_token = cmd.split()[2]  # uv, run, scripts/<x>.py, ...
        assert first_token.startswith("scripts/"), first_token
        script_rel = first_token[len("scripts/"):]
        script_path = scripts_dir / script_rel
        assert script_path.is_file(), (
            f"{sub}.{name} references missing script: {script_path}"
        )


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_skills_markdown_exists(sub: str, plugin_manifests: dict, plugin_dir: Path) -> None:
    manifest = plugin_manifests[sub]
    skills = manifest.get("skills", [])
    assert skills, f"{sub} declares no skills"
    for s in skills:
        path = plugin_dir / sub / s
        assert path.is_file(), f"{sub} skill {s!r} not found at {path}"
        # Frontmatter must have name and description.
        text = path.read_text()
        assert text.startswith("---"), f"{sub}/{s} missing YAML frontmatter"
        end = text.find("\n---", 3)
        assert end != -1, f"{sub}/{s} unterminated frontmatter"
        front = text[3:end]
        assert "name:" in front, f"{sub}/{s} frontmatter missing 'name:'"
        assert "description:" in front, f"{sub}/{s} frontmatter missing 'description:'"


@pytest.mark.parametrize("sub", SUBPLUGINS)
def test_skill_mirror_in_skills_plugins(sub: str, plugin_dir: Path) -> None:
    """Per the existing repo convention (see skills/plugins/huggingface/),
    each sub-plugin's skills/<id>.md must be byte-identical-mirrored at
    skills/plugins/<id>/<id>.md.
    """
    src = plugin_dir / sub / "skills" / f"{sub}.md"
    dst = plugin_dir.parent.parent / "skills" / "plugins" / sub / f"{sub}.md"
    assert src.is_file(), f"missing source: {src}"
    assert dst.is_file(), f"missing mirror: {dst}"
    assert src.read_bytes() == dst.read_bytes(), f"mirror not byte-identical: {sub}"


def test_package_json_includes_all_subplugins(plugin_manifests: dict) -> None:
    pkg = plugin_manifests["__package__"]
    files = set(pkg["files"])
    for sub in SUBPLUGINS:
        assert f"{sub}/" in files, f"package.json files missing {sub}/"


def test_scienceskillscommon_exists(plugin_dir: Path) -> None:
    """The shared scienceskillscommon package must live at the package root
    so the PEP 723 sources path `../../scienceskillscommon` resolves from
    each sub-plugin's scripts/ dir.
    """
    sc = plugin_dir / "scienceskillscommon"
    assert sc.is_dir(), f"missing scienceskillscommon at {sc}"
    assert (sc / "__init__.py").is_file()
    assert (sc / "http_client.py").is_file()
    assert (sc / "pyproject.toml").is_file()


def test_sync_script_exists_and_is_executable(plugin_dir: Path) -> None:
    sync = plugin_dir / "sync-from-bundle.sh"
    assert sync.is_file(), f"missing {sync}"
    import os
    import stat
    mode = sync.stat().st_mode
    assert mode & stat.S_IXUSR, f"{sync} is not executable by owner"
