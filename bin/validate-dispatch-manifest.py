#!/usr/bin/env python3
"""Validate a QUINTE dispatch manifest."""

from __future__ import annotations

import argparse
import importlib.util
import sys
from pathlib import Path
from typing import Any


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load module at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ROOT = Path(__file__).resolve().parents[1]
DISPATCH = load_module("quinte_dispatch_phase", ROOT / "bin" / "quinte-dispatch-phase.py")


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate a QUINTE dispatch manifest")
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--check-executables", action="store_true", help="also verify prompt, archive, and command executables")
    args = parser.parse_args()

    try:
        manifest_path = args.manifest.resolve()
        manifest = DISPATCH.load_json(manifest_path)
        errors = DISPATCH.validate_manifest(manifest)
        if args.check_executables and not errors:
            errors.extend(DISPATCH.executable_errors(manifest_path, manifest))
    except ValueError as exc:
        print(f"[QUINTE] ERROR: {exc}", file=sys.stderr)
        return 2

    if errors:
        for error in errors:
            print(f"[QUINTE] ERROR: {error}", file=sys.stderr)
        return 2

    print("[QUINTE] dispatch manifest valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
