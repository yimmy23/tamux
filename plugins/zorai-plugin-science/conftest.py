# Shared pytest configuration for zorai-plugin-science tests.
#
# This file is auto-discovered by pytest from any tests/ subdirectory under
# plugins/zorai-plugin-science/. It:
#   1. Makes the vendored scienceskillscommon package importable.
#   2. Adds each sub-plugin's scripts/ dir to sys.path so its CLI script
#      can be imported as a module (the scripts are guarded with
#      `if __name__ == "__main__":` blocks so they're safe to import).
#   3. Re-exports a `script_module` and `invoke_cli` fixture used by
#      per-sub-plugin test files.
#
# Run from the plugin root:
#   cd plugins/zorai-plugin-science
#   python3 -m venv .venv
#   .venv/bin/pip install pytest
#   .venv/bin/pytest tests/ -v
#
# (The vendored scienceskillscommon is pure-stdlib, so no extra runtime
# deps are needed beyond pytest. Sub-plugins that hit the network at
# runtime are tested via monkeypatching http_client in the per-sub-plugin
# test files.)
#
# IMPORTANT: do NOT leave .venv inside the plugin tree when publishing.
# The zorai plugin installer copies the plugin directory verbatim, and a
# ~50+ MB venv bloatsthe install. Delete .venv/ before `zorai plugin add`.

from __future__ import annotations

import importlib
import importlib.util
import json
import sys
from pathlib import Path

import pytest


# Plugin package root. The conftest.py sits at <plugin>/conftest.py so
# this file's parent IS the plugin root.
PLUGIN_DIR = Path(__file__).resolve().parent
# Vendored scienceskillscommon, shared by every sub-plugin.
SCICOMMON_SRC = PLUGIN_DIR / "scienceskillscommon"

# Sub-plugin name -> scripts dir.
SUBPLUGIN_SCRIPTS: dict[str, Path] = {
    # Full sub-plugins (hand-written plugin.json with named commands).
    "alphagenome": PLUGIN_DIR / "alphagenome" / "scripts",
    "alphafold-database": PLUGIN_DIR / "alphafold-database" / "scripts",
    "uniprot": PLUGIN_DIR / "uniprot" / "scripts",
    "clinvar": PLUGIN_DIR / "clinvar" / "scripts",
    "chembl": PLUGIN_DIR / "chembl" / "scripts",
    "openalex": PLUGIN_DIR / "openalex" / "scripts",
    "ensembl": PLUGIN_DIR / "ensembl" / "scripts",
    "reactome": PLUGIN_DIR / "reactome" / "scripts",
    "gnomad": PLUGIN_DIR / "gnomad" / "scripts",
    "pdb-database": PLUGIN_DIR / "pdb-database" / "scripts",
    "ncbi-sequence-fetch": PLUGIN_DIR / "ncbi-sequence-fetch" / "scripts",
    # Long-tail sub-plugins (stub plugin.json with a single `run` command).
    "clinical-trials-database":     PLUGIN_DIR / "clinical-trials-database" / "scripts",
    "dbsnp-database":               PLUGIN_DIR / "dbsnp-database" / "scripts",
    "embl-ebi-ols":                 PLUGIN_DIR / "embl-ebi-ols" / "scripts",
    "encode-ccres-database":        PLUGIN_DIR / "encode-ccres-database" / "scripts",
    "foldseek-structural-search":   PLUGIN_DIR / "foldseek-structural-search" / "scripts",
    "gtex-database":                PLUGIN_DIR / "gtex-database" / "scripts",
    "human-protein-atlas-database": PLUGIN_DIR / "human-protein-atlas-database" / "scripts",
    "interpro-database":            PLUGIN_DIR / "interpro-database" / "scripts",
    "jaspar-database":              PLUGIN_DIR / "jaspar-database" / "scripts",
    "literature-search-arxiv":      PLUGIN_DIR / "literature-search-arxiv" / "scripts",
    "literature-search-biorxiv":    PLUGIN_DIR / "literature-search-biorxiv" / "scripts",
    "literature-search-europepmc":  PLUGIN_DIR / "literature-search-europepmc" / "scripts",
    "ncbi-sequence-fetch":          PLUGIN_DIR / "ncbi-sequence-fetch" / "scripts",
    "openfda-database":             PLUGIN_DIR / "openfda-database" / "scripts",
    "opentargets-database":         PLUGIN_DIR / "opentargets-database" / "scripts",
    "pdb-database":                 PLUGIN_DIR / "pdb-database" / "scripts",
    "protein-sequence-msa":         PLUGIN_DIR / "protein-sequence-msa" / "scripts",
    "protein-sequence-similarity-search": PLUGIN_DIR / "protein-sequence-similarity-search" / "scripts",
    "pubchem-database":             PLUGIN_DIR / "pubchem-database" / "scripts",
    "pubmed-database":              PLUGIN_DIR / "pubmed-database" / "scripts",
    "pymol":                        PLUGIN_DIR / "pymol" / "scripts",  # may be empty
    "quickgo-database":             PLUGIN_DIR / "quickgo-database" / "scripts",
    "string-database":              PLUGIN_DIR / "string-database" / "scripts",
    "ucsc-conservation-and-tfbs":   PLUGIN_DIR / "ucsc-conservation-and-tfbs" / "scripts",
    "unibind-database":             PLUGIN_DIR / "unibind-database" / "scripts",
}


def _ensure_scicommon_on_path() -> None:
    """Make the vendored scienceskillscommon importable.

    The scripts use `from science_skills.scienceskillscommon import http_client`.
    The deepmind package layout is:
      scienceskillscommon/
        __init__.py
        http_client.py
        pyproject.toml
        SKILL.md
    The deepmind scripts treat `science_skills` as a namespace package —
    it's a directory that contains `scienceskillscommon` as a sub-package.
    We rebuild that minimal layout (symlink scienceskillscommon into a
    parent named `science_skills`) under a private _test_pkgs/ dir, and
    put the parent of `science_skills` on sys.path.
    """
    pkg_root = PLUGIN_DIR / "_test_pkgs" / "science_skills"
    target_link = pkg_root / "scienceskillscommon"
    if target_link.is_dir() and not target_link.exists() or target_link.is_symlink():
        # Stale or broken link; remove.
        target_link.unlink()
    pkg_root.mkdir(parents=True, exist_ok=True)
    if not (pkg_root / "scienceskillscommon").exists():
        (pkg_root / "scienceskillscommon").symlink_to(
            SCICOMMON_SRC.resolve(), target_is_directory=True
        )
    dotenv_shim = pkg_root.parent / "dotenv.py"
    if not dotenv_shim.exists():
        dotenv_shim.write_text(
            "def load_dotenv(*args, **kwargs):\n"
            "    return False\n"
            "\n"
            "def find_dotenv(*args, **kwargs):\n"
            "    return ''\n"
        )
    sys.path.insert(0, str(pkg_root.parent))


_ensure_scicommon_on_path()


def _load_module(name: str, path: Path):
    """Load a Python source file as a module under `name`.

    The deepmind scripts have flat names (visualize_variant_effects.py,
    openalex_cli.py, etc.) but a few collide (analyze_ism.py,
    analyze_pae.py) so we namespace them by their sub-plugin to avoid
    name shadowing when the same test session imports two of them.
    """
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load spec for {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def plugin_dir() -> Path:
    """Path to plugins/zorai-plugin-science/."""
    return PLUGIN_DIR


@pytest.fixture
def scienceskillscommon_src() -> Path:
    """Path to the vendored scienceskillscommon/ source dir."""
    return SCICOMMON_SRC


@pytest.fixture
def load_subplugin_script():
    """Factory fixture: import a sub-plugin's CLI script as a module.

    Usage:
        def test_x(load_subplugin_script):
            mod = load_subplugin_script("chembl", "chembl_api.py")
            assert callable(mod.cmd_status)
    """
    def _loader(subplugin: str, script_filename: str):
        scripts_dir = SUBPLUGIN_SCRIPTS[subplugin]
        script_path = scripts_dir / script_filename
        # Namespace the module by sub-plugin to avoid name collisions
        # (multiple sub-plugins have analyze_*.py, lookup_*.py, etc.).
        module_name = f"_zorai_test_{subplugin.replace('-', '_')}_{script_filename[:-3]}"
        return _load_module(module_name, script_path)
    return _loader


@pytest.fixture
def invoke_cli():
    """Run a sub-plugin's CLI script as a subprocess (like hf-tools tests do).

    Usage:
        def test_help(invoke_cli):
            r = invoke_cli("chembl", "chembl_api.py", ["--help"])
            assert r.returncode == 0
            assert "ChEMBL" in r.stdout

    Sets PYTHONPATH so the subprocess can import the vendored
    `science_skills.scienceskillscommon` shim (the deepmind scripts do
    `from science_skills.scienceskillscommon import http_client`).
    """
    import os
    import subprocess

    pkgs_parent = str(PLUGIN_DIR / "_test_pkgs")

    def _runner(subplugin: str, script_filename: str, args, env=None):
        script_path = SUBPLUGIN_SCRIPTS[subplugin] / script_filename
        merged_env = {**os.environ, **(env or {})}
        # Prepend our shim's parent so the subprocess finds `science_skills`.
        existing = merged_env.get("PYTHONPATH", "")
        if existing:
            merged_env["PYTHONPATH"] = pkgs_parent + os.pathsep + existing
        else:
            merged_env["PYTHONPATH"] = pkgs_parent
        return subprocess.run(
            [sys.executable, str(script_path), *args],
            capture_output=True,
            text=True,
            env=merged_env,
        )
    return _runner


@pytest.fixture
def plugin_manifests() -> dict[str, dict]:
    """Load every sub-plugin's plugin.json from disk.

    Returns a dict keyed by sub-plugin name. Use this for cross-cutting
    manifest assertions (every command references a real script, every
    skill.md exists, etc.).
    """
    out: dict[str, dict] = {}
    for sub in SUBPLUGIN_SCRIPTS:
        path = PLUGIN_DIR / sub / "plugin.json"
        with path.open() as f:
            out[sub] = json.load(f)
    # Also load the package.json (root).
    with (PLUGIN_DIR / "package.json").open() as f:
        out["__package__"] = json.load(f)
    return out


@pytest.fixture
def fake_http_response():
    """Factory fixture: build a deepmind HttpResponse-shaped object for tests.

    Usage:
        def test_x(fake_http_response, monkeypatch):
            resp = fake_http_response({"foo": "bar"})
            monkeypatch.setattr(http_client.HttpClient, "fetch", lambda self, url, **kw: resp)
    """
    from science_skills.scienceskillscommon import http_client

    def _factory(body, status_code=200, content_encoding="identity"):
        import json as _json
        if isinstance(body, (dict, list)):
            data = _json.dumps(body).encode()
        elif isinstance(body, str):
            data = body.encode()
        else:
            data = body  # assume bytes
        return http_client.HttpResponse(
            data=data,
            status_code=status_code,
            headers={"content-encoding": content_encoding, "content-type": "application/json"},
            url="https://test.invalid/",
            encoding="utf-8",
        )
    return _factory
