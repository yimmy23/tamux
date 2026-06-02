"""Offline tests for the alphafold-database sub-plugin.

alphafold-database hits the public AlphaFold DB API
(https://alphafold.ebi.ac.uk/) using scienceskillscommon.http_client.
We can fully exercise it offline by monkeypatching the URL fetch.
"""


def test_alphafold_all_three_help(invoke_cli) -> None:
    for s in ("fetch_structure.py", "analyze_plddt.py", "analyze_pae.py"):
        r = invoke_cli("alphafold-database", s, ["--help"])
        assert r.returncode == 0, f"{s} --help failed: {r.stderr}"


def test_alphafold_fetch_structure_404_paths(load_subplugin_script, monkeypatch, capsys) -> None:
    """fetch_structure.py should print a friendly 404 message and exit 1
    when the upstream returns 404 for an unknown UniProt ID.
    """
    from science_skills.scienceskillscommon import http_client  # vendored scienceskillscommon.http_client

    mod = load_subplugin_script("alphafold-database", "fetch_structure.py")

    # Build a fake HttpError(404) and raise it from a stubbed fetch_json.
    def fake_fetch_json(self, url, **kwargs):
        raise http_client.HttpError(
            "404 Not Found",
            status_code=404,
            url=url,
            body=b'{"error": "not found"}',
        )

    monkeypatch.setattr(http_client.HttpClient, "fetch_json", fake_fetch_json)

    import argparse
    args = argparse.Namespace(uniprot_id="BOGUS_ID", output_dir="/tmp/af_offline_test")
    # Should sys.exit(1) after printing the friendly message.
    import pytest
    with pytest.raises(SystemExit) as exc:
        mod.fetch_structure("BOGUS_ID", "/tmp/af_offline_test")
    assert exc.value.code == 1
    captured = capsys.readouterr()
    assert "not found" in captured.out.lower() or "BOGUS_ID" in captured.out
