#!/usr/bin/env python3
"""Validate a QUINTE dispatch ledger."""

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
    parser = argparse.ArgumentParser(description="Validate a QUINTE dispatch ledger")
    parser.add_argument("ledger", type=Path)
    parser.add_argument("--check-files", action="store_true", help="verify referenced prompt, output, and archive files")
    args = parser.parse_args()

    try:
        ledger_path = args.ledger.resolve()
        ledger = DISPATCH.load_json(ledger_path)
        errors = DISPATCH.validate_ledger(ledger)
        if args.check_files and not errors:
            errors.extend(DISPATCH.validate_ledger_files(ledger_path, ledger))
    except ValueError as exc:
        print(f"[QUINTE] ERROR: {exc}", file=sys.stderr)
        return 2

    if errors:
        for error in errors:
            print(f"[QUINTE] ERROR: {error}", file=sys.stderr)
        return 2

    status = ledger["status"]
    if not ledger["phase_progression_allowed"]:
        print(f"[QUINTE] dispatch ledger valid; phase status is {status}", file=sys.stderr)
        return 1

    print(f"[QUINTE] dispatch ledger valid; phase status is {status}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
