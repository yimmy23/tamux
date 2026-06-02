"""Offline tests for the ensembl sub-plugin.

ensembl hits the public Ensembl REST API. We mock http_client
and verify a representative subcommand path.
"""

from __future__ import annotations

from science_skills.scienceskillscommon import http_client


def test_ensembl_help(invoke_cli) -> None:
    r = invoke_cli("ensembl", "ensembl_api.py", ["--help"])
    assert r.returncode == 0, r.stderr
    # All 10 subcommands must appear in --help.
    for sub in (
        "resolve-gene", "map-id", "get-sequence", "gene-summary",
        "transcripts", "canonical-tss", "transcript-structure",
        "protein-info", "protein-sequence", "vep",
    ):
        assert sub in r.stdout, f"ensembl --help missing subcommand {sub!r}"


def test_ensembl_resolve_gene_offline(load_subplugin_script, monkeypatch, capsys, fake_http_response) -> None:
    """Mock the Ensembl REST API for a symbol->ENSG lookup and verify
    the script prints the resolved ENSG IDs.
    """
    canned = [
        {
            "id": "ENSG00000141510",
            "display_name": "TP53",
            "biotype": "protein_coding",
            "description": "tumor protein p53",
        }
    ]
    resp = fake_http_response(canned)

    mod = load_subplugin_script("ensembl", "ensembl_api.py")
    # _get_client builds a fresh client per command; we need to patch
    # HttpClient.fetch_json so any client gets the canned response.
    monkeypatch.setattr(
        http_client.HttpClient, "fetch_json", lambda self, url, **kw: canned
    )

    import argparse
    args = argparse.Namespace(
        query="TP53",
        assembly=None,
        species=None,
        output=None,
    )
    mod.cmd_resolve_gene(args)
    out = capsys.readouterr().out
    assert "ENSG00000141510" in out
    assert "TP53" in out


def test_ensembl_skill_md_lists_all_ten_commands(plugin_manifests) -> None:
    """The skill.md must reference all 10 plugin.json commands by their
    name so the agent knows what to call.
    """
    from pathlib import Path
    manifest = plugin_manifests["ensembl"]
    cmds = set(manifest["commands"].keys())
    skill_path = Path(__file__).resolve().parents[1] / manifest["skills"][0]
    text = skill_path.read_text()
    for cmd in cmds:
        assert cmd in text, f"ensembl skill.md does not mention command {cmd!r}"


def test_ensembl_no_required_settings(plugin_manifests) -> None:
    """ensembl has no required auth — settings should be empty or absent."""
    settings = plugin_manifests["ensembl"].get("settings", {})
    assert not any(s.get("required") is True for s in settings.values()), (
        f"ensembl requires no auth; settings must not have required=True: {settings!r}"
    )
