"""Offline tests for the uniprot sub-plugin.

uniprot hits the public UniProt REST API. We mock the http_client seam
to return canned JSON and verify the wrapper processes it correctly.
"""

from __future__ import annotations

from science_skills.scienceskillscommon import http_client


def test_uniprot_help(invoke_cli) -> None:
    r = invoke_cli("uniprot", "uniprot_tools.py", ["--help"])
    assert r.returncode == 0, r.stderr
    for sub in ("search", "get", "map", "count", "sparql", "stream"):
        assert sub in r.stdout, f"uniprot --help missing subcommand {sub!r}"


def test_uniprot_search_offline(load_subplugin_script, monkeypatch, fake_http_response) -> None:
    """Mock the upstream API and verify uniprot search returns parsed records."""
    canned = {
        "results": [
            {
                "primaryAccession": "P04637",
                "uniProtkbId": "P53_HUMAN",
                "proteinDescription": {"recommendedName": {"fullName": {"value": "Cellular tumor antigen p53"}}},
                "organism": {"scientificName": "Homo sapiens", "taxonId": 9606},
            },
            {
                "primaryAccession": "P02340",
                "uniProtkbId": "P53_MOUSE",
                "proteinDescription": {"recommendedName": {"fullName": {"value": "Cellular tumor antigen p53"}}},
                "organism": {"scientificName": "Mus musculus", "taxonId": 10090},
            },
        ],
        "facets": [],
    }
    resp = fake_http_response(canned)
    monkeypatch.setattr(http_client.HttpClient, "fetch", lambda self, url, **kw: resp)

    mod = load_subplugin_script("uniprot", "uniprot_tools.py")
    pages = list(mod.search_proteins("p53", dataset="uniprotkb", limit=2))
    assert len(pages) == 1
    results = pages[0]["results"]
    assert len(results) == 2
    assert results[0]["primaryAccession"] == "P04637"
    assert results[0]["organism"]["scientificName"] == "Homo sapiens"
    assert results[1]["primaryAccession"] == "P02340"


def test_uniprot_get_offline(load_subplugin_script, monkeypatch, fake_http_response) -> None:
    """Mock the upstream API for get_entry and verify it returns the entry."""
    canned_entry = {
        "primaryAccession": "P04637",
        "uniProtkbId": "P53_HUMAN",
        "sequence": {"length": 393, "version": 3},
    }
    resp = fake_http_response(canned_entry)
    monkeypatch.setattr(http_client.HttpClient, "fetch", lambda self, url, **kw: resp)

    mod = load_subplugin_script("uniprot", "uniprot_tools.py")
    entry = mod.get_entry("P04637", dataset="uniprotkb")
    assert entry["primaryAccession"] == "P04637"
    assert entry["sequence"]["length"] == 393
