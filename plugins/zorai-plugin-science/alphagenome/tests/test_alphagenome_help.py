"""Offline tests for the alphagenome sub-plugin.

alphagenome's scripts all import the proprietary `alphagenome` PyPI
package (real Google Cloud API client) and `matplotlib`, so the only
offline verifications we can do are:

  1. Every script file exists (covered by test_plugin_manifests).
  2. Every script has parseable Python syntax (catches truncation /
     bad refactor at the sync step).
  3. The script files each have a __name__ guard.
  4. The skill.md references all 7 plugin.json commands.
  5. The plugin.json declares ALPHAGENOME_API_KEY as required + secret.

The heavy functional tests require the real alphagenome API key and
a displayable matplotlib backend, and so live in the deepmind-bundle
SKILL.md as the "Workflow Checklist" rather than here.
"""

from __future__ import annotations

import ast
from pathlib import Path

import pytest


SCRIPTS = [
    "visualize_variant_effects.py",
    "interpret_splicing.py",
    "analyze_ism.py",
    "lookup_gene_info.py",
    "resolve_ontology_terms.py",
    "generate_ontology_mapping.py",
    "visualize_genome_tracks.py",
]


@pytest.mark.parametrize("script", SCRIPTS)
def test_script_parses(plugin_dir: Path, script: str) -> None:
    """Every script must be valid Python (catches truncation at sync)."""
    path = plugin_dir / "alphagenome" / "scripts" / script
    assert path.is_file(), f"missing script: {path}"
    text = path.read_text()
    # ast.parse will raise SyntaxError on bad Python.
    ast.parse(text, filename=str(path))


@pytest.mark.parametrize("script", SCRIPTS)
def test_script_has_main_guard(plugin_dir: Path, script: str) -> None:
    """Every script must be guarded with `if __name__ == ...main...:`
    so it can be both executed and imported (for testing). Uses AST to
    avoid quote-style brittleness (the deepmind scripts use single quotes
    for the guard, we use double quotes elsewhere).
    """
    path = plugin_dir / "alphagenome" / "scripts" / script
    tree = ast.parse(path.read_text(), filename=str(path))
    found = False
    for node in ast.walk(tree):
        if not isinstance(node, ast.If):
            continue
        # Look for `if __name__ == '<something_main>'`.
        test = node.test
        if (
            isinstance(test, ast.Compare)
            and isinstance(test.left, ast.Name)
            and test.left.id == "__name__"
            and len(test.ops) == 1
            and isinstance(test.ops[0], ast.Eq)
            and len(test.comparators) == 1
            and isinstance(test.comparators[0], ast.Constant)
            and isinstance(test.comparators[0].value, str)
            and test.comparators[0].value.endswith("__main__")
        ):
            found = True
            break
    assert found, f"{script} is missing `if __name__ == ...main...: guard`"


def test_skill_md_lists_all_seven_commands(plugin_manifests) -> None:
    """The skill.md must reference all 7 plugin.json commands by their
    name so the agent knows what to call.
    """
    manifest = plugin_manifests["alphagenome"]
    cmds = set(manifest["commands"].keys())
    skill_rel = manifest["skills"][0]
    skill_path = Path(__file__).resolve().parents[1] / skill_rel
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"alphagenome skill.md does not mention command {cmd!r}"


def test_alphagenome_api_key_required(plugin_manifests) -> None:
    """ALPHAGENOME_API_KEY must be required (no anonymous AlphaGenome access)
    and stored as a secret (never echoed to the agent context).
    """
    settings = plugin_manifests["alphagenome"].get("settings", {})
    assert "ALPHAGENOME_API_KEY" in settings
    s = settings["ALPHAGENOME_API_KEY"]
    assert s["required"] is True, "alphagenome requires an API key"
    assert s["secret"] is True, "API key must be marked secret"
