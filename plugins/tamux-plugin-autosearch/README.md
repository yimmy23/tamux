# tamux-plugin-autosearch

AutoResearch workflow plugin for tamux.

This plugin is a daemon-native reference-style plugin in the same family as the Gmail / Google Calendar plugins, but instead of OAuth-backed HTTP endpoints it provides Python-backed local workflow commands for an AutoResearch workspace.

## Scope

Minimal workflow wrapper around a local AutoResearch checkout:
- validate workspace shape
- run `uv run prepare.py`
- run `uv run train.py`

## Installation

From a published package:

```bash
tamux plugin add tamux-plugin-autosearch
```

For local development, place the plugin under the daemon plugin directory or use the repo plugin directory as a source during development.

## Configuration

The plugin exposes minimal settings:
- `workspace_path`: default AutoResearch workspace path
- `default_command_timeout_sec`: optional timeout hint

## Commands

| Command | Description |
|---|---|
| `/autosearch.check` | Validate a local AutoResearch workspace |
| `/autosearch.prepare` | Run `uv run prepare.py` in the workspace |
| `/autosearch.train` | Run `uv run train.py` in the workspace |

## Runtime note

Python-backed plugin commands are emitted to the agent as a shell bootstrap plus the original slash-command arguments. For the MVP, the agent should bind the target workspace into the shell environment as `AUTOSEARCH_WORKSPACE` before execution, or substitute the explicit workspace path directly in the helper invocation.

## Notes

Artifacts remain in the target workspace, including:
- `run.log`
- `results.tsv`
- any upstream-generated files

The plugin reports status and command output; it does not vendor upstream AutoResearch.
