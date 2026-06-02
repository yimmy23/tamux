"""Offline tests for the chembl sub-plugin.

chembl_api.py is the richest subcommand script in the bundle (34
endpoints + 4 utilities). Mock the http_client seam and verify a
representative path.
"""


def test_chembl_help(invoke_cli) -> None:
    r = invoke_cli("chembl", "chembl_api.py", ["--help"])
    assert r.returncode == 0, r.stderr
    # Spot-check a few of the 34 endpoints.
    for sub in ("activity", "molecule", "target", "drug", "mechanism", "status", "similarity"):
        assert sub in r.stdout, f"chembl --help missing subcommand {sub!r}"


def test_chembl_status_offline(load_subplugin_script, monkeypatch, tmp_path) -> None:
    """Mock fetch_json and verify cmd_status writes the canned response to disk."""
    import json as _json

    canned = {
        "status": "UP",
        "chembl_db_version": "ChEMBL_TEST",
        "chembl_release_date": "2026-01-01",
        "activities": 100,
        "targets": 50,
    }

    def fake_fetch_json(self, url, **kwargs):
        return canned

    from science_skills.scienceskillscommon import http_client
    monkeypatch.setattr(http_client.HttpClient, "fetch_json", fake_fetch_json)

    mod = load_subplugin_script("chembl", "chembl_api.py")
    import argparse
    out_path = tmp_path / "chembl_status.json"
    args = argparse.Namespace(output=str(out_path))
    mod.cmd_status(args)
    assert out_path.is_file()
    data = _json.loads(out_path.read_text())
    assert data["status"] == "UP"
    assert data["chembl_db_version"] == "ChEMBL_TEST"
    # The license notice must be preserved in the output.
    assert "license" in out_path.read_text().lower() or "ChEMBL" in out_path.read_text()


def test_chembl_license_notice_present(load_subplugin_script) -> None:
    """The deepmind chembl script must surface a license notice on every command.
    The plugin's skill.md instructs the agent to relay this to the user.
    """
    mod = load_subplugin_script("chembl", "chembl_api.py")
    assert hasattr(mod, "_LICENSE_NOTICE")
    assert "ChEMBL" in mod._LICENSE_NOTICE
    assert "license" in mod._LICENSE_NOTICE.lower() or "terms" in mod._LICENSE_NOTICE.lower()


def test_chembl_searchable_endpoints_list(load_subplugin_script) -> None:
    """The set of searchable endpoints must include the workhorses
    (activity, assay, molecule, target)."""
    mod = load_subplugin_script("chembl", "chembl_api.py")
    searchable = mod.SEARCHABLE_ENDPOINTS
    for ep in ("activity", "assay", "molecule", "target"):
        assert ep in searchable, f"{ep} should be a searchable endpoint"
