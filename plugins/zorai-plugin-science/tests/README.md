# Tests

Offline pytest suite for `zorai-plugin-science`. No network calls — every
HTTP-touching script is exercised by monkeypatching the deepmind
`scienceskillscommon.http_client` seam, so the tests work in CI without
secrets, without an outbound connection, and without a daemon.

## Layout

```text
plugins/zorai-plugin-science/
  conftest.py                          # shared pytest fixtures (loads scienceskillscommon)
  requirements-dev.txt                 # just pytest
  tests/
    README.md                          # this file
    test_plugin_manifests.py           # cross-cutting plugin.json validation
    test_sync_script.py                # sync-from-bundle.sh smoke test
    alphagenome/tests/test_help.py
    alphafold-database/tests/test_help.py
    uniprot/tests/test_help.py
    uniprot/tests/test_search_offline.py
    clinvar/tests/test_help.py
    clinvar/tests/test_summary_offline.py
    chembl/tests/test_help.py
    chembl/tests/test_status_offline.py
    openalex/tests/test_help.py
    openalex/tests/test_resolve_offline.py
```

## Run

```bash
cd plugins/zorai-plugin-science

# One-time setup. .venv stays local; delete before publishing.
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt

# All tests, verbose.
.venv/bin/pytest tests/ -v

# One sub-plugin only.
.venv/bin/pytest tests/chembl/ -v

# One test.
.venv/bin/pytest tests/chembl/tests/test_status_offline.py -v
```

The plugin's vendored `scienceskillscommon/` is pure-stdlib (no third-party
runtime deps). Tests import it via a small symlink shim set up in
`conftest.py` — see the docstring there for why.

## Conventions

- **`invoke_cli` fixture** runs a script as a subprocess with the system
  Python. Use this for `--help` smoke tests and any test that exercises
  the script's argparse parser.
- **`load_subplugin_script` fixture** imports a script as a Python module
  (the scripts are guarded with `if __name__ == "__main__":` so this is
  safe). Use this when you need to call internal functions or monkeypatch
  the script's module-level `_CLIENT` to return canned responses.
- **No real network calls.** If a test needs HTTP, monkeypatch the
  script's `_CLIENT.fetch_json` (or the equivalent per-script seam) to
  return a canned response. See `chembl/tests/test_status_offline.py`
  for the canonical example.
- **Plugin manifest tests** live in `tests/test_plugin_manifests.py` and
  are cross-cutting: every `commands.<name>.python.command` is checked
  to start with `python scripts/`, every command-bearing sub-plugin must
  vendor `scienceskillscommon/`, and every `skills[0]` file must exist.

## Adding a new sub-plugin

1. Add the sub-plugin dir under `plugins/zorai-plugin-science/<name>/`
   with `plugin.json` and `skills/<name>.md`.
2. Add `<name>: PLUGIN_DIR / "<name>" / "scripts"` to `SUBPLUGIN_SCRIPTS`
   in `conftest.py`.
3. Add `<name>` to the `SKILL_DIRS` map in `sync-from-bundle.sh`.
4. Add `package.json` `files: [...]` entry for `<name>/`.
5. Add a `tests/<name>/tests/test_help.py` and one offline mock test.
6. Re-run the full suite; CI also catches regressions.

## CI

See `.github/workflows/zorai-plugin-science.yml`. CI runs the offline
test suite, smoke-tests script help output, then runs `sync-from-bundle.sh`
from a clean state and re-runs the tests to confirm idempotency.
