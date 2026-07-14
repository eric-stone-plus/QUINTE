#!/usr/bin/env python3
"""Dispatch a QUINTE phase through host-bound native CLI routes."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


MANIFEST_NON_AUTHORIZATION = "QUINTE dispatch manifests do not authorize action, substitute parties, or modify routing rules."
LEDGER_NON_AUTHORIZATION = "QUINTE dispatch ledgers do not authorize action, substitute parties, or advance blocked phases."

PARTY_IDS = ["Party A", "Party B", "Party C", "Party D", "Party E"]
PHASES = {"R1", "R2", "R3"}
STATUSES = {"complete", "blocked", "degraded"}
PARTY_STATUSES = {"succeeded", "blocked", "degraded"}
ERROR_CLASSES = {
    "auth",
    "rate_limit",
    "timeout",
    "interrupted_recoverable",
    "deprecated",
    "empty_output",
    "invalid_output",
    "dispatcher_exception",
    "unknown",
}
RETRYABLE_ERRORS = {
    "rate_limit",
    "timeout",
    "interrupted_recoverable",
    "empty_output",
    "invalid_output",
    "dispatcher_exception",
    "unknown",
}
BLOCKING_ERRORS = {"auth", "deprecated"}

TOP_LEVEL_FIELDS = {
    "dispatch_manifest_version",
    "run_id",
    "phase",
    "question",
    "prompt_ref",
    "archive_dir",
    "timeout_seconds",
    "max_attempts",
    "retry_backoff_seconds",
    "prompt_shrink_char_limit",
    "substitution_policy",
    "parties",
    "auditor_b",
    "non_authorization",
}
ROUTE_FIELDS = {"id", "route_id", "command", "required"}
LEDGER_FIELDS = {
    "dispatch_ledger_version",
    "run_id",
    "phase",
    "status",
    "phase_progression_allowed",
    "inputs",
    "archive_dir",
    "summary",
    "parties",
    "blocking_failures",
    "non_authorization",
}
LEDGER_INPUT_FIELDS = {"manifest_ref", "prompt_ref"}
SUMMARY_FIELDS = {
    "required_count",
    "succeeded_count",
    "failed_count",
    "blocked_count",
    "degraded_count",
    "max_attempts",
}
LEDGER_PARTY_FIELDS = {
    "id",
    "route_id",
    "required",
    "command_hash",
    "status",
    "attempt_count",
    "last_error_class",
    "output_ref",
    "attempts",
}
ATTEMPT_FIELDS = {
    "attempt",
    "route_id",
    "prompt_ref",
    "prompt_sha256",
    "stdout_ref",
    "stderr_ref",
    "exit_code",
    "timed_out",
    "output_bytes",
    "error_class",
    "retryable",
    "started_at",
    "ended_at",
    "duration_ms",
}
BLOCKING_FAILURE_FIELDS = {"party_id", "route_id", "error_class", "reason"}


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as exc:
        raise ValueError(f"cannot read {path}: {exc}") from exc
    except json.JSONDecodeError as exc:
        raise ValueError(f"{path} is invalid JSON: {exc.msg}") from exc
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def write_json(value: dict[str, Any], *, pretty: bool) -> str:
    return json.dumps(value, ensure_ascii=False, indent=2 if pretty else None, sort_keys=True)


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def resolve_ref(base_file: Path, ref: str | None) -> Path | None:
    if ref is None or "://" in ref:
        return None
    ref_path = Path(ref)
    if ref_path.is_absolute():
        return ref_path.resolve()
    return (base_file.parent / ref_path).resolve()


def is_nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and value.strip() != ""


def is_bool(value: Any) -> bool:
    return isinstance(value, bool)


def is_positive_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def is_nonnegative_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def is_nonnegative_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and value >= 0


def validate_fields(name: str, value: Any, fields: set[str], errors: list[str]) -> dict[str, Any]:
    if not isinstance(value, dict):
        errors.append(f"{name} must be an object")
        return {}
    unknown = sorted(set(value) - fields)
    if unknown:
        errors.append(f"{name} has unknown fields: {', '.join(unknown)}")
    missing = sorted(fields - set(value))
    if missing:
        errors.append(f"{name} is missing fields: {', '.join(missing)}")
    return value


def validate_command(name: str, value: Any, errors: list[str]) -> list[str]:
    if not isinstance(value, list) or len(value) == 0:
        errors.append(f"{name} must be a non-empty array of command tokens")
        return []
    command: list[str] = []
    for index, item in enumerate(value):
        if not is_nonempty_string(item):
            errors.append(f"{name}[{index}] must be a non-empty string")
        else:
            command.append(item)
    return command


def validate_route(name: str, value: Any, errors: list[str], *, expected_id: str | None = None) -> dict[str, Any]:
    route = validate_fields(name, value, ROUTE_FIELDS, errors)
    if not route:
        return {}
    if not is_nonempty_string(route.get("id")):
        errors.append(f"{name}.id must be a non-empty string")
    elif expected_id is not None and route.get("id") != expected_id:
        errors.append(f"{name}.id must be {expected_id}")
    if not is_nonempty_string(route.get("route_id")):
        errors.append(f"{name}.route_id must be a non-empty string")
    validate_command(f"{name}.command", route.get("command"), errors)
    if route.get("required") is not True:
        errors.append(f"{name}.required must be true")
    return route


def validate_manifest(manifest: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(manifest, dict):
        return ["dispatch manifest must be an object"]

    validate_fields("manifest", manifest, TOP_LEVEL_FIELDS, errors)
    if manifest.get("dispatch_manifest_version") != "0.1.1":
        errors.append("dispatch_manifest_version must be 0.1.1")
    for field in ("run_id", "question", "prompt_ref", "archive_dir"):
        if not is_nonempty_string(manifest.get(field)):
            errors.append(f"{field} must be a non-empty string")
    if manifest.get("phase") not in PHASES:
        errors.append("phase must be R1, R2, or R3")
    if not is_positive_int(manifest.get("timeout_seconds")):
        errors.append("timeout_seconds must be a positive integer")
    if not is_positive_int(manifest.get("max_attempts")):
        errors.append("max_attempts must be a positive integer")
    if not is_nonnegative_number(manifest.get("retry_backoff_seconds")):
        errors.append("retry_backoff_seconds must be a non-negative number")
    if not is_positive_int(manifest.get("prompt_shrink_char_limit")):
        errors.append("prompt_shrink_char_limit must be a positive integer")
    if manifest.get("substitution_policy") != "same_route_only":
        errors.append("substitution_policy must be same_route_only")
    if manifest.get("non_authorization") != MANIFEST_NON_AUTHORIZATION:
        errors.append("non_authorization text is invalid")

    phase = manifest.get("phase")
    parties = manifest.get("parties")
    parsed_parties: list[dict[str, Any]] = []
    if not isinstance(parties, list):
        errors.append("parties must be an array")
    elif phase in {"R1", "R2"} and len(parties) != 5:
        errors.append("parties must contain exactly five R1/R2 party bindings")
    elif phase == "R3" and len(parties) not in {0, 5}:
        errors.append("R3 parties must be empty or contain the archived Party A through Party E bindings")
    else:
        seen_ids: set[str] = set()
        seen_routes: set[str] = set()
        for index, party in enumerate(parties):
            expected_id = PARTY_IDS[index] if index < len(PARTY_IDS) else None
            parsed = validate_route(f"parties[{index}]", party, errors, expected_id=expected_id)
            if not parsed:
                continue
            parsed_parties.append(parsed)
            party_id = parsed.get("id")
            route_id = parsed.get("route_id")
            if isinstance(party_id, str):
                if party_id in seen_ids:
                    errors.append(f"parties[{index}].id is duplicated")
                seen_ids.add(party_id)
            if isinstance(route_id, str):
                if route_id in seen_routes:
                    errors.append(f"parties[{index}].route_id is duplicated")
                seen_routes.add(route_id)
        if len(parties) == 5 and {party.get("id") for party in parsed_parties} != set(PARTY_IDS):
            errors.append("parties must bind Party A through Party E exactly once")

    auditor = manifest.get("auditor_b")
    if auditor is not None:
        validate_route("auditor_b", auditor, errors, expected_id="Auditor B")
    if phase == "R3":
        if not isinstance(auditor, dict) or auditor.get("required") is not True:
            errors.append("R3 dispatch requires auditor_b.required true")

    return errors


def dispatch_command(manifest_path: Path, command: list[str]) -> list[str]:
    """Resolve the executable token exactly as preflight resolves it."""
    executable = command[0]
    if "/" not in executable:
        return command
    executable_path = Path(executable)
    if not executable_path.is_absolute():
        executable_path = (manifest_path.parent / executable_path).resolve()
    return [str(executable_path), *command[1:]]


def executable_errors(manifest_path: Path, manifest: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    routes = dispatch_targets(manifest) if manifest.get("phase") in PHASES else []
    for route in routes:
        if not isinstance(route, dict):
            continue
        command = route.get("command")
        if not isinstance(command, list) or not command:
            continue
        executable = command[0]
        if not isinstance(executable, str) or executable.strip() == "":
            continue
        if "/" in executable:
            executable_path = Path(executable)
            if not executable_path.is_absolute():
                executable_path = (manifest_path.parent / executable_path).resolve()
            if not executable_path.exists():
                errors.append(f"{route.get('id')} executable does not exist: {executable}")
            elif not os.access(executable_path, os.X_OK):
                errors.append(f"{route.get('id')} executable is not executable: {executable}")
        elif shutil.which(executable) is None:
            errors.append(f"{route.get('id')} executable is not on PATH: {executable}")
    prompt_path = resolve_ref(manifest_path, manifest.get("prompt_ref"))
    if prompt_path is None:
        errors.append("prompt_ref must be a local file reference")
    elif not prompt_path.exists():
        errors.append(f"prompt_ref does not exist: {prompt_path}")
    archive_dir = resolve_ref(manifest_path, manifest.get("archive_dir"))
    if archive_dir is None:
        errors.append("archive_dir must be a local file reference")
    else:
        try:
            archive_dir.mkdir(parents=True, exist_ok=True)
            probe = archive_dir / ".quinte-preflight-probe"
            probe.write_text("ok", encoding="utf-8")
            probe.unlink()
        except OSError as exc:
            errors.append(f"archive_dir is not writable: {archive_dir}: {exc}")
    return errors


def dispatch_targets(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    if manifest["phase"] in {"R1", "R2"}:
        return list(manifest["parties"])
    auditor = manifest.get("auditor_b")
    return [auditor] if isinstance(auditor, dict) else []


def slug(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9]+", "-", value.strip()).strip("-").lower()
    return cleaned or "party"


def sha256_text(text: str) -> str:
    return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()


def command_hash(command: list[str]) -> str:
    payload = json.dumps(command, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return sha256_text(payload)


def shorten_prompt(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    return text[:limit].rstrip() + "\n\n[Prompt shortened after dispatch failure.]"


def has_task_restatement(stdout_text: str) -> bool:
    first_content_line = next((line.strip() for line in stdout_text.splitlines() if line.strip()), "")
    return first_content_line.startswith("TASK:")


def classify_result(exit_code: int | None, timed_out: bool, stdout_text: str, stderr_text: str, output_bytes: int) -> str | None:
    combined = f"{stdout_text}\n{stderr_text}".lower()
    if timed_out:
        return "timeout"
    if exit_code in {130, 143, -2, -15}:
        return "interrupted_recoverable"
    if re.search(r"\b(429|rate limit|rate_limit|too many requests|quota)\b", combined):
        return "rate_limit"
    if re.search(r"\b(401|403|unauthorized|unauthorised|authentication|credential|api key|login required|permission denied)\b", combined):
        return "auth"
    if re.search(r"\b(deprecated|removed|not supported|unsupported model|model not found|route not found|command not found)\b", combined):
        return "deprecated"
    if exit_code == 127:
        return "deprecated"
    if exit_code == 0 and output_bytes == 0:
        return "empty_output"
    if exit_code == 0:
        if not has_task_restatement(stdout_text):
            return "invalid_output"
        return None
    if exit_code == 2:
        return "auth"
    return "unknown"


def run_attempt(
    manifest_path: Path,
    manifest: dict[str, Any],
    archive_dir: Path,
    prompt_text: str,
    route: dict[str, Any],
    attempt_number: int,
) -> dict[str, Any]:
    party_slug = slug(route["id"])
    prompt_dir = archive_dir / "prompts"
    output_dir = archive_dir / "outputs"
    prompt_dir.mkdir(parents=True, exist_ok=True)
    output_dir.mkdir(parents=True, exist_ok=True)

    prompt_path = prompt_dir / f"{party_slug}-attempt-{attempt_number}.txt"
    stdout_path = output_dir / f"{party_slug}-attempt-{attempt_number}.out"
    stderr_path = output_dir / f"{party_slug}-attempt-{attempt_number}.err"
    prompt_path.write_text(prompt_text, encoding="utf-8")

    env = os.environ.copy()
    env.update(
        {
            "QUINTE_RUN_ID": manifest["run_id"],
            "QUINTE_PHASE": manifest["phase"],
            "QUINTE_PARTY_ID": route["id"],
            "QUINTE_ROUTE_ID": route["route_id"],
            "QUINTE_PROMPT_FILE": str(prompt_path),
            "QUINTE_OUTPUT_FILE": str(stdout_path),
            "QUINTE_ARCHIVE_DIR": str(archive_dir),
            "QUINTE_ATTEMPT": str(attempt_number),
        }
    )

    started = utc_now()
    start_time = time.monotonic()
    exit_code: int | None = None
    timed_out = False
    error_override: str | None = None
    command = dispatch_command(manifest_path, route["command"])
    try:
        with prompt_path.open("rb") as stdin_file, stdout_path.open("wb") as stdout_file, stderr_path.open("wb") as stderr_file:
            completed = subprocess.run(
                command,
                stdin=stdin_file,
                stdout=stdout_file,
                stderr=stderr_file,
                timeout=manifest["timeout_seconds"],
                env=env,
                cwd=str(archive_dir),
                check=False,
            )
        exit_code = completed.returncode
    except subprocess.TimeoutExpired:
        timed_out = True
    except OSError as exc:
        stderr_path.write_text(str(exc), encoding="utf-8")
        exit_code = 127
    except Exception as exc:
        stderr_path.write_text(f"{type(exc).__name__}: {exc}", encoding="utf-8")
        error_override = "dispatcher_exception"
    duration_ms = int((time.monotonic() - start_time) * 1000)
    ended = utc_now()

    stdout_text = stdout_path.read_text(encoding="utf-8", errors="replace") if stdout_path.exists() else ""
    stderr_text = stderr_path.read_text(encoding="utf-8", errors="replace") if stderr_path.exists() else ""
    output_bytes = stdout_path.stat().st_size if stdout_path.exists() else 0
    error_class = error_override or classify_result(exit_code, timed_out, stdout_text, stderr_text, output_bytes)
    retryable = error_class in RETRYABLE_ERRORS

    return {
        "attempt": attempt_number,
        "route_id": route["route_id"],
        "prompt_ref": str(prompt_path),
        "prompt_sha256": sha256_text(prompt_text),
        "stdout_ref": str(stdout_path),
        "stderr_ref": str(stderr_path),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "output_bytes": output_bytes,
        "error_class": error_class,
        "retryable": retryable,
        "started_at": started,
        "ended_at": ended,
        "duration_ms": duration_ms,
    }


def dispatch_route(
    manifest_path: Path,
    manifest: dict[str, Any],
    archive_dir: Path,
    base_prompt: str,
    route: dict[str, Any],
) -> dict[str, Any]:
    attempts: list[dict[str, Any]] = []
    prompt_text = base_prompt
    max_attempts = manifest["max_attempts"]
    for attempt_number in range(1, max_attempts + 1):
        attempt = run_attempt(manifest_path, manifest, archive_dir, prompt_text, route, attempt_number)
        attempts.append(attempt)
        error_class = attempt["error_class"]
        if error_class is None:
            return party_ledger(route, attempts, "succeeded")
        if error_class in BLOCKING_ERRORS:
            return party_ledger(route, attempts, "blocked")
        if attempt_number < max_attempts:
            if error_class in {"timeout", "empty_output", "invalid_output"}:
                prompt_text = shorten_prompt(base_prompt, manifest["prompt_shrink_char_limit"])
            if error_class == "rate_limit" and manifest["retry_backoff_seconds"] > 0:
                time.sleep(float(manifest["retry_backoff_seconds"]))
            continue
    return party_ledger(route, attempts, "degraded")


def party_ledger(route: dict[str, Any], attempts: list[dict[str, Any]], status: str) -> dict[str, Any]:
    last = attempts[-1]
    output_ref = last["stdout_ref"] if status == "succeeded" else None
    return {
        "id": route["id"],
        "route_id": route["route_id"],
        "required": route["required"],
        "command_hash": command_hash(route["command"]),
        "status": status,
        "attempt_count": len(attempts),
        "last_error_class": last["error_class"],
        "output_ref": output_ref,
        "attempts": attempts,
    }


def dispatcher_exception_party_ledger(
    manifest: dict[str, Any],
    archive_dir: Path,
    base_prompt: str,
    route: dict[str, Any],
    exc: BaseException,
) -> dict[str, Any]:
    party_slug = slug(route["id"])
    prompt_dir = archive_dir / "prompts"
    output_dir = archive_dir / "outputs"
    prompt_dir.mkdir(parents=True, exist_ok=True)
    output_dir.mkdir(parents=True, exist_ok=True)

    prompt_path = prompt_dir / f"{party_slug}-dispatcher-exception.txt"
    stdout_path = output_dir / f"{party_slug}-dispatcher-exception.out"
    stderr_path = output_dir / f"{party_slug}-dispatcher-exception.err"
    prompt_path.write_text(base_prompt, encoding="utf-8")
    stdout_path.write_text("", encoding="utf-8")
    stderr_path.write_text(f"{type(exc).__name__}: {exc}", encoding="utf-8")
    now = utc_now()
    attempt = {
        "attempt": 1,
        "route_id": route["route_id"],
        "prompt_ref": str(prompt_path),
        "prompt_sha256": sha256_text(base_prompt),
        "stdout_ref": str(stdout_path),
        "stderr_ref": str(stderr_path),
        "exit_code": None,
        "timed_out": False,
        "output_bytes": 0,
        "error_class": "dispatcher_exception",
        "retryable": True,
        "started_at": now,
        "ended_at": now,
        "duration_ms": 0,
    }
    return party_ledger(route, [attempt], "degraded")


def build_ledger(manifest_path: Path, manifest: dict[str, Any]) -> dict[str, Any]:
    prompt_path = resolve_ref(manifest_path, manifest["prompt_ref"])
    archive_dir = resolve_ref(manifest_path, manifest["archive_dir"])
    if prompt_path is None or archive_dir is None:
        raise ValueError("prompt_ref and archive_dir must be local file references")
    archive_dir.mkdir(parents=True, exist_ok=True)
    try:
        probe = archive_dir / ".quinte-write-probe"
        probe.write_text("ok", encoding="utf-8")
        probe.unlink()
    except OSError as exc:
        raise ValueError(f"archive_dir is not writable: {archive_dir}: {exc}") from exc

    base_prompt = prompt_path.read_text(encoding="utf-8")
    routes = dispatch_targets(manifest)
    if not routes:
        raise ValueError("no dispatch targets for phase")

    with concurrent.futures.ThreadPoolExecutor(max_workers=len(routes)) as executor:
        future_routes = [
            (executor.submit(dispatch_route, manifest_path, manifest, archive_dir, base_prompt, route), route)
            for route in routes
        ]
        parties = []
        for future, route in future_routes:
            try:
                parties.append(future.result())
            except Exception as exc:
                parties.append(dispatcher_exception_party_ledger(manifest, archive_dir, base_prompt, route, exc))

    succeeded_count = sum(1 for party in parties if party["status"] == "succeeded")
    blocked_count = sum(1 for party in parties if party["status"] == "blocked")
    degraded_count = sum(1 for party in parties if party["status"] == "degraded")
    failed_count = blocked_count + degraded_count
    if blocked_count > 0:
        status = "blocked"
    elif degraded_count > 0:
        status = "degraded"
    else:
        status = "complete"

    blocking_failures = []
    for party in parties:
        if party["status"] == "succeeded":
            continue
        blocking_failures.append(
            {
                "party_id": party["id"],
                "route_id": party["route_id"],
                "error_class": party["last_error_class"],
                "reason": f"{party['id']} ended {party['status']} after {party['attempt_count']} attempt(s)",
            }
        )

    return {
        "dispatch_ledger_version": "0.1.1",
        "run_id": manifest["run_id"],
        "phase": manifest["phase"],
        "status": status,
        "phase_progression_allowed": status == "complete",
        "inputs": {
            "manifest_ref": str(manifest_path),
            "prompt_ref": str(prompt_path),
        },
        "archive_dir": str(archive_dir),
        "summary": {
            "required_count": len(routes),
            "succeeded_count": succeeded_count,
            "failed_count": failed_count,
            "blocked_count": blocked_count,
            "degraded_count": degraded_count,
            "max_attempts": manifest["max_attempts"],
        },
        "parties": parties,
        "blocking_failures": blocking_failures,
        "non_authorization": LEDGER_NON_AUTHORIZATION,
    }


def validate_attempt(index: int, value: Any, errors: list[str]) -> dict[str, Any]:
    attempt = validate_fields(f"attempts[{index}]", value, ATTEMPT_FIELDS, errors)
    if not attempt:
        return {}
    if not is_positive_int(attempt.get("attempt")):
        errors.append(f"attempts[{index}].attempt must be a positive integer")
    for field in ("route_id", "prompt_ref", "prompt_sha256", "stdout_ref", "stderr_ref", "started_at", "ended_at"):
        if not is_nonempty_string(attempt.get(field)):
            errors.append(f"attempts[{index}].{field} must be a non-empty string")
    if attempt.get("exit_code") is not None and not isinstance(attempt.get("exit_code"), int):
        errors.append(f"attempts[{index}].exit_code must be an integer or null")
    if not is_bool(attempt.get("timed_out")):
        errors.append(f"attempts[{index}].timed_out must be a boolean")
    if not is_nonnegative_int(attempt.get("output_bytes")):
        errors.append(f"attempts[{index}].output_bytes must be a non-negative integer")
    if attempt.get("error_class") is not None and attempt.get("error_class") not in ERROR_CLASSES:
        errors.append(f"attempts[{index}].error_class is invalid")
    if not is_bool(attempt.get("retryable")):
        errors.append(f"attempts[{index}].retryable must be a boolean")
    if not is_nonnegative_int(attempt.get("duration_ms")):
        errors.append(f"attempts[{index}].duration_ms must be a non-negative integer")
    return attempt


def validate_ledger_party(index: int, value: Any, errors: list[str]) -> dict[str, Any]:
    party = validate_fields(f"parties[{index}]", value, LEDGER_PARTY_FIELDS, errors)
    if not party:
        return {}
    for field in ("id", "route_id", "command_hash", "status"):
        if not is_nonempty_string(party.get(field)):
            errors.append(f"parties[{index}].{field} must be a non-empty string")
    if party.get("status") not in PARTY_STATUSES:
        errors.append(f"parties[{index}].status is invalid")
    if party.get("required") is not True:
        errors.append(f"parties[{index}].required must be true")
    if not is_positive_int(party.get("attempt_count")):
        errors.append(f"parties[{index}].attempt_count must be a positive integer")
    if party.get("last_error_class") is not None and party.get("last_error_class") not in ERROR_CLASSES:
        errors.append(f"parties[{index}].last_error_class is invalid")
    if party.get("status") == "succeeded":
        if party.get("last_error_class") is not None:
            errors.append(f"parties[{index}].last_error_class must be null when succeeded")
        if not is_nonempty_string(party.get("output_ref")):
            errors.append(f"parties[{index}].output_ref must be provided when succeeded")
    elif party.get("output_ref") is not None:
        errors.append(f"parties[{index}].output_ref must be null when not succeeded")

    attempts = party.get("attempts")
    parsed_attempts: list[dict[str, Any]] = []
    if not isinstance(attempts, list) or len(attempts) == 0:
        errors.append(f"parties[{index}].attempts must be a non-empty array")
    else:
        for attempt_index, attempt in enumerate(attempts, start=1):
            parsed = validate_attempt(attempt_index, attempt, errors)
            if parsed:
                parsed_attempts.append(parsed)
                if parsed.get("attempt") != attempt_index:
                    errors.append(f"parties[{index}] attempt sequence must be consecutive starting at 1")
        if isinstance(party.get("attempt_count"), int) and party["attempt_count"] != len(parsed_attempts):
            errors.append(f"parties[{index}].attempt_count differs from attempts length")
        for attempt in parsed_attempts:
            if attempt.get("route_id") != party.get("route_id"):
                errors.append(f"parties[{index}] attempt route_id differs from party route_id")
            error_class = attempt.get("error_class")
            retryable = attempt.get("retryable")
            if error_class is None and retryable is not False:
                errors.append(f"parties[{index}] successful attempt must not be retryable")
            if error_class in RETRYABLE_ERRORS and retryable is not True:
                errors.append(f"parties[{index}] retryable error class must set retryable true")
            if error_class in BLOCKING_ERRORS and retryable is not False:
                errors.append(f"parties[{index}] blocking error class must set retryable false")
        if parsed_attempts:
            last = parsed_attempts[-1]
            if party.get("last_error_class") != last.get("error_class"):
                errors.append(f"parties[{index}].last_error_class differs from final attempt")
            if party.get("status") == "succeeded":
                if party.get("output_ref") != last.get("stdout_ref"):
                    errors.append(f"parties[{index}].output_ref must equal final stdout_ref when succeeded")
                if not isinstance(last.get("output_bytes"), int) or last.get("output_bytes", 0) <= 0:
                    errors.append(f"parties[{index}] succeeded with empty final output")
            if party.get("status") == "blocked" and last.get("error_class") not in BLOCKING_ERRORS:
                errors.append(f"parties[{index}] blocked status requires a blocking final error class")
            if party.get("status") == "degraded" and last.get("error_class") not in RETRYABLE_ERRORS:
                errors.append(f"parties[{index}] degraded status requires exhausted retryable final error")
    return party


def validate_ledger(ledger: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(ledger, dict):
        return ["dispatch ledger must be an object"]

    validate_fields("ledger", ledger, LEDGER_FIELDS, errors)
    if ledger.get("dispatch_ledger_version") != "0.1.1":
        errors.append("dispatch_ledger_version must be 0.1.1")
    if not is_nonempty_string(ledger.get("run_id")):
        errors.append("run_id must be a non-empty string")
    if ledger.get("phase") not in PHASES:
        errors.append("phase must be R1, R2, or R3")
    if ledger.get("status") not in STATUSES:
        errors.append("status is invalid")
    if not is_bool(ledger.get("phase_progression_allowed")):
        errors.append("phase_progression_allowed must be a boolean")
    elif ledger.get("phase_progression_allowed") != (ledger.get("status") == "complete"):
        errors.append("phase_progression_allowed is inconsistent with status")
    if not is_nonempty_string(ledger.get("archive_dir")):
        errors.append("archive_dir must be a non-empty string")
    if ledger.get("non_authorization") != LEDGER_NON_AUTHORIZATION:
        errors.append("non_authorization text is invalid")

    inputs = validate_fields("inputs", ledger.get("inputs"), LEDGER_INPUT_FIELDS, errors)
    if inputs:
        for field in LEDGER_INPUT_FIELDS:
            if not is_nonempty_string(inputs.get(field)):
                errors.append(f"inputs.{field} must be a non-empty string")

    summary = validate_fields("summary", ledger.get("summary"), SUMMARY_FIELDS, errors)
    if summary:
        for field in SUMMARY_FIELDS:
            if not is_nonnegative_int(summary.get(field)):
                errors.append(f"summary.{field} must be a non-negative integer")

    parsed_parties: list[dict[str, Any]] = []
    parties = ledger.get("parties")
    if not isinstance(parties, list) or len(parties) == 0:
        errors.append("parties must be a non-empty array")
    else:
        for index, party in enumerate(parties, start=1):
            parsed = validate_ledger_party(index, party, errors)
            if parsed:
                parsed_parties.append(parsed)
        if ledger.get("phase") in {"R1", "R2"} and [party.get("id") for party in parsed_parties] != PARTY_IDS:
            errors.append("R1/R2 ledgers must include Party A through Party E in order")
        if ledger.get("phase") == "R3" and [party.get("id") for party in parsed_parties] != ["Auditor B"]:
            errors.append("R3 ledgers must include Auditor B")

    failures = ledger.get("blocking_failures")
    parsed_failures: list[dict[str, Any]] = []
    if not isinstance(failures, list):
        errors.append("blocking_failures must be an array")
    else:
        for index, failure in enumerate(failures, start=1):
            parsed = validate_fields(f"blocking_failures[{index}]", failure, BLOCKING_FAILURE_FIELDS, errors)
            if not parsed:
                continue
            parsed_failures.append(parsed)
            for field in ("party_id", "route_id", "reason"):
                if not is_nonempty_string(parsed.get(field)):
                    errors.append(f"blocking_failures[{index}].{field} must be a non-empty string")
            if parsed.get("error_class") not in ERROR_CLASSES:
                errors.append(f"blocking_failures[{index}].error_class is invalid")

    if summary and parsed_parties:
        succeeded_count = sum(1 for party in parsed_parties if party.get("status") == "succeeded")
        blocked_count = sum(1 for party in parsed_parties if party.get("status") == "blocked")
        degraded_count = sum(1 for party in parsed_parties if party.get("status") == "degraded")
        failed_count = blocked_count + degraded_count
        expected_status = "blocked" if blocked_count else "degraded" if degraded_count else "complete"
        if summary.get("required_count") != len(parsed_parties):
            errors.append("summary.required_count differs from parties length")
        if summary.get("succeeded_count") != succeeded_count:
            errors.append("summary.succeeded_count differs from party statuses")
        if summary.get("blocked_count") != blocked_count:
            errors.append("summary.blocked_count differs from party statuses")
        if summary.get("degraded_count") != degraded_count:
            errors.append("summary.degraded_count differs from party statuses")
        if summary.get("failed_count") != failed_count:
            errors.append("summary.failed_count differs from party statuses")
        if ledger.get("status") != expected_status:
            errors.append(f"status should be {expected_status}, got {ledger.get('status')}")
        if len(parsed_failures) != failed_count:
            errors.append("blocking_failures length differs from failed party count")
        expected_failures = {
            (party.get("id"), party.get("route_id"), party.get("last_error_class"))
            for party in parsed_parties
            if party.get("status") != "succeeded"
        }
        actual_failures = {
            (failure.get("party_id"), failure.get("route_id"), failure.get("error_class"))
            for failure in parsed_failures
        }
        if actual_failures != expected_failures:
            errors.append("blocking_failures do not match failed party statuses")

    return errors


def validate_ledger_files(ledger_path: Path, ledger: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    for ref in (ledger.get("inputs", {}).get("manifest_ref"), ledger.get("inputs", {}).get("prompt_ref")):
        path = resolve_ref(ledger_path, ref)
        if path is not None and not path.exists():
            errors.append(f"referenced input does not exist: {path}")
    archive_dir = Path(ledger.get("archive_dir", ""))
    if not archive_dir.exists():
        errors.append(f"archive_dir does not exist: {archive_dir}")
    for party in ledger.get("parties", []):
        if not isinstance(party, dict):
            continue
        output_ref = party.get("output_ref")
        if isinstance(output_ref, str):
            output_path = resolve_ref(ledger_path, output_ref)
            if output_path is None or not output_path.exists():
                errors.append(f"{party.get('id')} output_ref does not exist: {output_ref}")
        for attempt in party.get("attempts", []):
            if not isinstance(attempt, dict):
                continue
            for field in ("prompt_ref", "stdout_ref", "stderr_ref"):
                ref = attempt.get(field)
                path = resolve_ref(ledger_path, ref) if isinstance(ref, str) else None
                if path is None or not path.exists():
                    errors.append(f"{party.get('id')} attempt {attempt.get('attempt')} {field} does not exist: {ref}")
                    continue
                if field == "stdout_ref" and isinstance(attempt.get("output_bytes"), int):
                    actual_size = path.stat().st_size
                    if actual_size != attempt["output_bytes"]:
                        errors.append(f"{party.get('id')} attempt {attempt.get('attempt')} output_bytes differs from stdout size")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Dispatch a QUINTE phase through host-bound native CLI routes")
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--pretty", action="store_true", help="pretty-print JSON output")
    args = parser.parse_args()

    try:
        manifest_path = args.manifest.resolve()
        manifest = load_json(manifest_path)
        errors = validate_manifest(manifest)
        if not errors:
            errors.extend(executable_errors(manifest_path, manifest))
        if errors:
            for error in errors:
                print(f"[QUINTE] ERROR: {error}", file=sys.stderr)
            return 2
        ledger = build_ledger(manifest_path, manifest)
        ledger_errors = validate_ledger(ledger)
        if ledger_errors:
            for error in ledger_errors:
                print(f"[QUINTE] ERROR: {error}", file=sys.stderr)
            return 2
    except ValueError as exc:
        print(f"[QUINTE] ERROR: {exc}", file=sys.stderr)
        return 2

    print(write_json(ledger, pretty=args.pretty))
    if ledger["phase_progression_allowed"]:
        return 0
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
