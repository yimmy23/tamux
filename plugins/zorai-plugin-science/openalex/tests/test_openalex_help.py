"""Offline tests for the openalex sub-plugin.

openalex hits the public OpenAlex REST API. We mock http_client and
verify a representative subcommand path.
"""

from __future__ import annotations

from science_skills.scienceskillscommon import http_client


def test_openalex_help(invoke_cli) -> None:
    r = invoke_cli("openalex", "openalex_cli.py", ["--help"])
    assert r.returncode == 0, r.stderr
    for sub in ("resolve", "get", "download-pdf", "filter", "rate-limit"):
        assert sub in r.stdout, f"openalex --help missing subcommand {sub!r}"


def test_openalex_resolve_offline(load_subplugin_script, monkeypatch, capsys, fake_http_response) -> None:
    """Mock the OpenAlex API for an author resolve query and verify
    the script prints the resolved author ID.
    """
    canned = {
        "meta": {"count": 1, "per_page": 10},
        "results": [
            {
                "id": "https://openalex.org/A5023888391",
                "display_name": "Yann LeCun",
                "works_count": 502,
                "cited_by_count": 199999,
            }
        ],
    }
    resp = fake_http_response(canned)

    mod = load_subplugin_script("openalex", "openalex_cli.py")
    # The script uses a module-level _API_CLIENT (an HttpClient instance).
    # Patch .fetch on the instance, not the class — fetch_with_retry calls
    # _API_CLIENT.fetch(full_url, max_retries=...) directly.
    monkeypatch.setattr(mod._API_CLIENT, "fetch", lambda url, **kw: resp)

    import argparse
    args = argparse.Namespace(
        entity_type="authors",
        query="Yann LeCun",
        per_page=10,
        search=None,
        filter=None,
        sort=None,
        group_by=None,
        page=None,
        select=None,
        id=None,
        api_key=None,
    )
    mod.handle_resolve(args)
    out = capsys.readouterr().out
    assert "A5023888391" in out
    assert "Yann LeCun" in out


def test_openalex_optional_api_key_setting(plugin_manifests) -> None:
    """OPENALEX_API_KEY is optional (raises rate limit, not required)."""
    settings = plugin_manifests["openalex"].get("settings", {})
    assert "OPENALEX_API_KEY" in settings
    assert settings["OPENALEX_API_KEY"]["required"] is False


def test_openalex_download_pdf_marked_billing_sensitive(plugin_manifests) -> None:
    """The download-pdf command description should call out the $0.01 cost.
    The agent uses this to decide whether to confirm with the user.
    """
    cmd = plugin_manifests["openalex"]["commands"]["download-pdf"]
    assert "$0.01" in cmd["description"] or "0.01" in cmd["description"]
    assert "cost" in cmd["description"].lower() or "charges" in cmd["description"].lower()
