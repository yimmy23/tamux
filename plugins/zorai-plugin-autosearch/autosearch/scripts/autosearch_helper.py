#!/usr/bin/env python3
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REQUIRED_FILES = ["program.md", "train.py", "prepare.py", "pyproject.toml"]


def fail(message: str, code: int = 1):
    print(message, file=sys.stderr)
    raise SystemExit(code)


def resolve_workspace(argv: list[str]) -> Path:
    if len(argv) < 3:
        fail("usage: autosearch_helper.py <check|prepare|train> <workspace_path>")
    workspace = Path(argv[2]).expanduser().resolve()
    if not workspace.exists():
        fail(f"workspace does not exist: {workspace}")
    if not workspace.is_dir():
        fail(f"workspace is not a directory: {workspace}")
    return workspace


def validate_workspace(workspace: Path):
    missing = [name for name in REQUIRED_FILES if not (workspace / name).exists()]
    if missing:
        fail(
            "workspace is not a valid AutoResearch repo; missing: " + ", ".join(missing)
        )


def ensure_uv():
    if shutil.which("uv") is None:
        fail("required executable not found: uv")


def run_command(workspace: Path, args: list[str]):
    proc = subprocess.run(args, cwd=str(workspace), check=False)
    raise SystemExit(proc.returncode)


def main(argv: list[str]):
    if len(argv) < 2:
        fail("usage: autosearch_helper.py <check|prepare|train> <workspace_path>")
    mode = argv[1]
    workspace = resolve_workspace(argv)
    validate_workspace(workspace)

    if mode == "check":
        payload = {
            "ok": True,
            "workspace": str(workspace),
            "required_files": REQUIRED_FILES,
        }
        print(json.dumps(payload, indent=2))
        return

    ensure_uv()

    if mode == "prepare":
        run_command(workspace, ["uv", "run", "prepare.py"])
    elif mode == "train":
        run_command(workspace, ["uv", "run", "train.py"])
    else:
        fail(f"unsupported mode: {mode}")


if __name__ == "__main__":
    main(sys.argv)
