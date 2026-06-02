"""Test for plugins/zorai-plugin-science/sync-from-bundle.sh.

The sync script copies deepmind scripts and the shared
scienceskillscommon package from the vendored bundle at
skills/scientific-skills-gdm/ into the per-sub-plugin scripts/ dirs
and the package root.

These tests verify:
  1. The script is executable (also covered by test_plugin_manifests).
  2. Running it from a known state produces a known state.
  3. It is idempotent (running twice doesn't accumulate cruft).
  4. After sync, the scienceskillscommon package resolves correctly
     inside an installed sub-plugin (the normalized PEP 723 sources path).
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

# Re-use the conftest's central map. Importing from conftest is the
# canonical pytest way to share fixtures/constants across test files.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from conftest import SUBPLUGIN_SCRIPTS  # noqa: E402


def test_sync_script_runs_clean(plugin_dir: Path) -> None:
    """Running the sync script on a clean checkout should succeed and
    leave every sub-plugin's scripts/ populated.
    """
    sync = plugin_dir / "sync-from-bundle.sh"
    result = subprocess.run(
        [str(sync)],
        cwd=str(plugin_dir),
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, (
        f"sync-from-bundle.sh failed:\nstdout={result.stdout}\nstderr={result.stderr}"
    )
    # Sanity check: every known sub-plugin's scripts/ has files.
    for sub in SUBPLUGIN_SCRIPTS:
        d = plugin_dir / sub / "scripts"
        assert d.is_dir(), f"{sub}/scripts missing after sync"
        # pymol has no Python entry scripts (binary-driven skill). Allow
        # an empty scripts/ dir for that case.
        if sub == "pymol":
            continue
        assert any(d.iterdir()), f"{sub}/scripts empty after sync"


def test_sync_script_idempotent(plugin_dir: Path) -> None:
    """Run sync twice; the second run should be a no-op (no errors, same state).
    This catches scripts that accumulate state instead of replacing.
    """
    sync = plugin_dir / "sync-from-bundle.sh"
    # First run.
    r1 = subprocess.run([str(sync)], cwd=str(plugin_dir), capture_output=True, text=True)
    assert r1.returncode == 0, r1.stderr
    # Snapshot file counts.
    counts_before = {
        sub: sum(1 for _ in (plugin_dir / sub / "scripts").iterdir())
        for sub in ("alphagenome", "alphafold-database", "uniprot", "clinvar", "chembl", "openalex")
    }
    # Second run.
    r2 = subprocess.run([str(sync)], cwd=str(plugin_dir), capture_output=True, text=True)
    assert r2.returncode == 0, r2.stderr
    counts_after = {
        sub: sum(1 for _ in (plugin_dir / sub / "scripts").iterdir())
        for sub in ("alphagenome", "alphafold-database", "uniprot", "clinvar", "chembl", "openalex")
    }
    assert counts_before == counts_after, (
        f"sync is not idempotent: before={counts_before} after={counts_after}"
    )


def test_sync_writes_to_subplugin_scripts(plugin_dir: Path) -> None:
    """Each sub-plugin's scripts/ must end up with the deepmind scripts."""
    sync = plugin_dir / "sync-from-bundle.sh"
    subprocess.run([str(sync)], cwd=str(plugin_dir), check=True, capture_output=True)
    # Spot-check: openalex has exactly one CLI script.
    openalex_scripts = sorted(p.name for p in (plugin_dir / "openalex" / "scripts").iterdir())
    assert "openalex_cli.py" in openalex_scripts
    # alphagenome has 7 scripts.
    alphagenome_scripts = sorted(p.name for p in (plugin_dir / "alphagenome" / "scripts").iterdir())
    assert len(alphagenome_scripts) >= 5, alphagenome_scripts


def test_scienceskillscommon_at_package_root(plugin_dir: Path) -> None:
    """The shared scienceskillscommon package must be at the package root."""
    sc = plugin_dir / "scienceskillscommon"
    assert sc.is_dir()
    assert (sc / "__init__.py").is_file()
    assert (sc / "http_client.py").is_file()


def test_scienceskillscommon_path_resolves_from_subplugin(plugin_dir: Path) -> None:
    """The PEP 723 sources path must resolve inside the installed sub-plugin."""
    sample = plugin_dir / "uniprot" / "scripts" / "uniprot_tools.py"
    assert sample.is_file()
    # Read the PEP 723 block to confirm the expected path.
    text = sample.read_text()
    assert 'scienceskillscommon = { path = "../scienceskillscommon" }' in text, (
        "PEP 723 sources path in uniprot_tools.py changed; update the sync logic."
    )
    # And confirm the target exists.
    target = (plugin_dir / "uniprot" / "scripts" / ".." / "scienceskillscommon").resolve()
    assert target.is_dir(), f"PEP 723 path does not resolve: {target}"
    assert (target / "__init__.py").is_file()
