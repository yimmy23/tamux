---
name: autosearch
description: >
  Local AutoResearch workflow plugin. Validate a workspace, run prepare, and run train.
---

# Autosearch Plugin

You have access to the **Autosearch plugin**.

## Commands

| Command | What it does |
|---|---|
| `/autosearch.check <workspace_path>` | Validate that a path looks like an AutoResearch workspace |
| `/autosearch.prepare <workspace_path>` | Run `uv run prepare.py` in the workspace |
| `/autosearch.train <workspace_path>` | Run `uv run train.py` in the workspace |

## Required workspace files

A valid workspace contains:
- `program.md`
- `train.py`
- `prepare.py`
- `pyproject.toml`

## Execution rule for MVP

When translating a slash command into execution, bind the chosen workspace path into `AUTOSEARCH_WORKSPACE` before running the plugin shell bootstrap, or replace the helper-script workspace argument directly with the explicit path.

Examples:
- `AUTOSEARCH_WORKSPACE=~/git/autoresearch`
- then run the plugin bootstrap for `check`, `prepare`, or `train`

## Usage rules

- Prefer explicit workspace paths in the slash command arguments for the MVP.
- Treat the workspace as the source of truth for outputs.
- Do not imply that the plugin vendors or embeds upstream AutoResearch.
- If validation fails, explain which required files are missing.
- If execution fails, surface the command failure clearly and do not invent recovery.
