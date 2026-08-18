#!/usr/bin/env python3
"""Small, fail-closed supervisor for an ordered QUINTE campaign.

This is an *outer* host helper.  It never starts a lane, resumes a run, or
edits QUINTE state.  It only uses ``quinte host`` and performs one observation
cycle per invocation.  The default is a dry run; launching requires the
explicit ``--execute`` flag.

The plan and state files are deliberately separate from QUINTE's state root.
The plan is immutable for the lifetime of a supervisor state file.  A terminal
``completed`` run is admitted only after a separate ``host inspect`` receipt
proves result integrity, current/actionable result semantics, the pinned state
root, and the pinned runtime digest.  ``degraded``, failed, ambiguous, or
malformed observations stop the campaign and atomically create ``HALTED``.

Example (all paths and the digest are intentionally explicit)::

    python3 scripts/contest_supervisor.py \
      --plan /abs/campaign/plan.json \
      --state /abs/campaign/supervisor-state.json \
      --home /abs/quinte-state \
      --quinte-bin /abs/bin/quinte \
      --runtime-sha256 sha256:<64 hex digits> \
      --json                 # inspect / dry-run

    ... same command with --execute to admit exactly one next sequence ...

The command is intentionally one-shot.  A scheduler may invoke it again
later, but this process never sleeps in a polling loop.
"""

from __future__ import annotations

import argparse
import contextlib
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from typing import Any, Callable, Iterator

try:  # Unix is the primary deployment target; keep the module importable on Windows.
    import fcntl
except ImportError:  # pragma: no cover - exercised only on Windows
    fcntl = None  # type: ignore[assignment]


PLAN_SCHEMA = "quinte.contest.plan.v1"
STATE_SCHEMA = "quinte.contest.supervisor-state.v1"
HALTED_SCHEMA = "quinte.contest.supervisor-halted.v1"
HOST_RECEIPT_VERSION = "1.0"

TERMINAL_STATUSES = {
    "completed",
    "degraded",
    "failed",
    "failed_policy",
    "cancelled",
}
ACTIVE_STATUSES = {
    "queued",
    "preflight",
    "r1_running",
    "r1_gate",
    "r2_packet",
    "r2_running",
    "r2_gate",
    "r3_cc",
    "waiting_primary_arbiter",
    "merging",
    "cancelling",
}
ALL_STATUSES = TERMINAL_STATUSES | ACTIVE_STATUSES
DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
UUIDV7_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
)


class SupervisorError(RuntimeError):
    """A caller-visible error which is not necessarily campaign corruption."""


class SafetyError(SupervisorError):
    """An observation that must fail-stop the campaign."""


class HaltedError(SupervisorError):
    """The campaign is already halted, or was halted during this invocation."""


class LockBusyError(SupervisorError):
    """Another supervisor instance owns the lock."""


@dataclass(frozen=True)
class Invocation:
    """Lossless-enough result of one machine-oriented CLI call.

    A nonzero process outcome and a parseable JSON envelope are kept separate:
    ``host inspect`` legitimately returns nonzero for a degraded run.
    """

    command: tuple[str, ...]
    returncode: int | None
    stdout: str
    stderr: str
    payload: dict[str, Any] | None
    parse_error: str | None = None
    timed_out: bool = False
    execution_error: str | None = None

    def record(self) -> dict[str, Any]:
        return {
            "command": list(self.command),
            "returncode": self.returncode,
            "timed_out": self.timed_out,
            "execution_error": self.execution_error,
            "parse_error": self.parse_error,
            "payload": self.payload,
            "stderr": self.stderr[-2000:] if self.stderr else "",
            # Do not persist unbounded adapter output in the supervisor ledger.
            "stdout": self.stdout[-2000:] if self.payload is None else None,
        }

    @property
    def output_ok(self) -> bool:
        return (
            not self.timed_out
            and self.execution_error is None
            and self.parse_error is None
        )


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _digest_bytes(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def _digest_file(path: Path) -> str:
    try:
        return _digest_bytes(path.read_bytes())
    except OSError as exc:
        raise SafetyError(f"cannot hash {path}: {exc}") from exc


def _require_digest(value: Any, label: str) -> str:
    if not isinstance(value, str) or not DIGEST_RE.fullmatch(value):
        raise SafetyError(f"{label} must be canonical sha256:<64 lowercase hex>")
    return value


def _absolute(value: Any, label: str, *, must_exist: bool = False) -> Path:
    if not isinstance(value, str) or not value:
        raise SafetyError(f"{label} must be a non-empty absolute path")
    path = Path(value)
    if not path.is_absolute():
        raise SafetyError(f"{label} must be absolute: {value!r}")
    try:
        return path.resolve(strict=must_exist)
    except OSError as exc:
        raise SafetyError(f"cannot resolve {label} {value!r}: {exc}") from exc


def _read_json(path: Path, label: str) -> tuple[Any, bytes]:
    try:
        raw = path.read_bytes()
        value = json.loads(raw.decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SafetyError(f"{label} is unreadable or malformed ({path}): {exc}") from exc
    return value, raw


def _atomic_write(path: Path, payload: bytes) -> None:
    """Write bytes via a fsynced sibling and atomic replacement."""

    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        with contextlib.suppress(OSError):
            os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        # A directory fsync closes the usual rename-durability gap on Unix.
        if hasattr(os, "O_DIRECTORY"):
            with contextlib.suppress(OSError):
                directory_fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
                try:
                    os.fsync(directory_fd)
                finally:
                    os.close(directory_fd)
    except Exception:
        with contextlib.suppress(FileNotFoundError):
            os.unlink(temporary)
        raise


def _atomic_json(path: Path, value: Any) -> None:
    _atomic_write(
        path,
        (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
    )


@contextlib.contextmanager
def _exclusive_lock(path: Path) -> Iterator[None]:
    """Acquire a non-blocking, process-wide supervisor lock."""

    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as handle:
        if fcntl is not None:
            try:
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise LockBusyError(f"another supervisor owns {path}") from exc
        else:  # pragma: no cover - Windows fallback
            import msvcrt

            try:
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError as exc:
                raise LockBusyError(f"another supervisor owns {path}") from exc
        try:
            yield
        finally:
            if fcntl is not None:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            else:  # pragma: no cover - Windows fallback
                import msvcrt

                with contextlib.suppress(OSError):
                    handle.seek(0)
                    msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)


def _canonical_brief_digest(path: Path) -> str:
    """Match QUINTE's compact serde serialization for Brief 1.1."""

    value, _ = _read_json(path, f"brief {path}")
    if not isinstance(value, dict):
        raise SafetyError(f"brief is not a JSON object: {path}")
    allowed = {
        "brief_version",
        "question",
        "context",
        "evidence_roots",
        "snapshot_ignore",
        "attachments",
        "action_scope",
        "affected_paths",
        "action_binding_sha256",
    }
    unknown = sorted(set(value) - allowed)
    if unknown:
        raise SafetyError(f"brief has unknown fields: {', '.join(unknown)}")
    if value.get("brief_version") != "1.1":
        raise SafetyError("brief_version must be 1.1")
    if not isinstance(value.get("question"), str) or not value["question"].strip():
        raise SafetyError("brief question must be a non-empty string")
    for field in ("evidence_roots", "snapshot_ignore", "attachments", "affected_paths"):
        if field in value and not isinstance(value[field], list):
            raise SafetyError(f"brief {field} must be an array")
    # Keep Rust Brief's declaration order and materialize serde defaults.
    canonical = {
        "brief_version": value.get("brief_version"),
        "question": value.get("question"),
        "context": value.get("context"),
        "evidence_roots": value.get("evidence_roots", []),
        "snapshot_ignore": value.get("snapshot_ignore", []),
        "attachments": value.get("attachments", []),
        "action_scope": value.get("action_scope"),
        "affected_paths": value.get("affected_paths", []),
        "action_binding_sha256": value.get("action_binding_sha256"),
    }
    return _digest_bytes(
        json.dumps(canonical, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    )


def _valid_run_id(value: Any) -> bool:
    return isinstance(value, str) and bool(UUIDV7_RE.fullmatch(value))


class Supervisor:
    """One-shot ordered campaign supervisor.

    ``runner`` is injectable solely for offline tests; production uses
    ``_run_command`` and the pinned executable.
    """

    def __init__(
        self,
        *,
        plan: Path,
        state: Path,
        home: Path,
        quinte_bin: Path,
        runtime_sha256: str,
        execute: bool = False,
        runner: Callable[[list[str], dict[str, str], float], Invocation] | None = None,
    ) -> None:
        self.plan_path = _absolute(str(plan), "plan", must_exist=True)
        self.state_path = _absolute(str(state), "state")
        self.home = _absolute(str(home), "QUINTE_HOME", must_exist=True)
        self.quinte_bin = _absolute(str(quinte_bin), "QUINTE_BIN", must_exist=True)
        if not self.quinte_bin.is_file():
            raise SafetyError(f"QUINTE_BIN is not a regular file: {self.quinte_bin}")
        if not self.home.is_dir():
            raise SafetyError(f"QUINTE_HOME is not a directory: {self.home}")
        self.runtime_sha256 = _require_digest(runtime_sha256, "runtime digest")
        self.execute = execute
        self.runner = runner or _run_command
        self.lock_path = self.state_path.with_name(self.state_path.name + ".lock")
        self.halted_path = self.state_path.with_name("HALTED")

    def _halt(self, reason: str, detail: str, *, state: dict[str, Any] | None = None) -> None:
        if not self.halted_path.exists():
            _atomic_json(
                self.halted_path,
                {
                    "schema": HALTED_SCHEMA,
                    "halted_at": _now(),
                    "reason": reason,
                    "detail": detail[:4000],
                    "plan": str(self.plan_path),
                    "state": str(self.state_path),
                    "state_root": str(self.home),
                    "runtime_sha256": self.runtime_sha256,
                    "last_state": state,
                },
            )
        raise HaltedError(f"campaign halted: {reason}: {detail}")

    def _guard_binary(self) -> None:
        actual = _digest_file(self.quinte_bin)
        if actual != self.runtime_sha256:
            raise SafetyError(
                f"pinned QUINTE binary digest drifted: expected {self.runtime_sha256}, got {actual}"
            )

    def _load_plan(self) -> tuple[dict[str, Any], str, list[dict[str, Any]]]:
        value, raw = _read_json(self.plan_path, "campaign plan")
        if not isinstance(value, dict) or value.get("schema") != PLAN_SCHEMA:
            raise SafetyError(f"campaign plan schema must be {PLAN_SCHEMA}")
        unknown_plan = sorted(set(value) - {"schema", "state_root", "runtime_sha256", "entries", "campaign_id", "created_at"})
        if unknown_plan:
            raise SafetyError(f"campaign plan has unknown fields: {', '.join(unknown_plan)}")
        declared_home = value.get("state_root")
        if declared_home is None:
            raise SafetyError("campaign plan must pin state_root explicitly")
        if _absolute(declared_home, "plan state_root") != self.home:
            raise SafetyError("plan state_root does not equal pinned QUINTE_HOME")
        declared_runtime = _require_digest(value.get("runtime_sha256"), "plan runtime_sha256")
        if declared_runtime != self.runtime_sha256:
            raise SafetyError("plan runtime_sha256 does not equal the pinned runtime digest")
        entries = value.get("entries")
        if not isinstance(entries, list):
            raise SafetyError("campaign plan entries must be an array")
        normalized: list[dict[str, Any]] = []
        for index, item in enumerate(entries, 1):
            if not isinstance(item, dict) or type(item.get("sequence")) is not int or item.get("sequence") != index:
                raise SafetyError("campaign plan sequences must be contiguous starting at 1")
            unknown_entry = sorted(set(item) - {"sequence", "brief", "brief_sha256", "batch_id", "label"})
            if unknown_entry:
                raise SafetyError(f"plan entry {index} has unknown fields: {', '.join(unknown_entry)}")
            brief = _absolute(item.get("brief"), f"plan entry {index} brief", must_exist=True)
            if not brief.is_file():
                raise SafetyError(f"plan entry {index} brief is not a regular file: {brief}")
            expected = item.get("brief_sha256")
            if expected is None:
                expected = _canonical_brief_digest(brief)
            else:
                expected = _require_digest(expected, f"plan entry {index} brief_sha256")
            actual = _canonical_brief_digest(brief)
            if actual != expected:
                raise SafetyError(
                    f"plan entry {index} brief changed: expected {expected}, got {actual}"
                )
            normalized.append(
                {
                    **item,
                    "sequence": index,
                    "brief": str(brief),
                    "brief_sha256": expected,
                }
            )
        if not normalized:
            raise SafetyError("campaign plan must contain at least one entry")
        return value, _digest_bytes(raw), normalized

    def _initial_state(self, plan_digest: str) -> dict[str, Any]:
        return {
            "schema": STATE_SCHEMA,
            "created_at": _now(),
            "updated_at": _now(),
            "plan_sha256": plan_digest,
            "state_root": str(self.home),
            "quinte_bin": str(self.quinte_bin),
            "runtime_sha256": self.runtime_sha256,
            "last_accepted_sequence": 0,
            "next_sequence": 1,
            "active": None,
            "acceptances": [],
            "last_observation": None,
        }

    def _validate_state(self, state: Any, plan_digest: str, count: int) -> dict[str, Any]:
        if not isinstance(state, dict) or state.get("schema") != STATE_SCHEMA:
            raise SafetyError(f"supervisor state schema must be {STATE_SCHEMA}")
        for key, expected in (
            ("plan_sha256", plan_digest),
            ("state_root", str(self.home)),
            ("quinte_bin", str(self.quinte_bin)),
            ("runtime_sha256", self.runtime_sha256),
        ):
            if state.get(key) != expected:
                raise SafetyError(f"supervisor state binding drifted for {key}")
        last = state.get("last_accepted_sequence")
        nxt = state.get("next_sequence")
        if type(last) is not int or type(nxt) is not int or last < 0 or nxt != last + 1:
            raise SafetyError("supervisor state sequence cursor is invalid")
        if last > count:
            raise SafetyError("supervisor state accepted sequence exceeds plan")
        acceptances = state.get("acceptances")
        if not isinstance(acceptances, list):
            raise SafetyError("supervisor acceptance history is not contiguous")
        accepted_sequences: list[int] = []
        accepted_run_ids: set[str] = set()
        for index, item in enumerate(acceptances, 1):
            if not isinstance(item, dict) or type(item.get("sequence")) is not int or item.get("sequence") != index:
                raise SafetyError("supervisor acceptance history has a sequence gap")
            accepted_sequences.append(index)
            accepted_run_id = item.get("run_id")
            if not _valid_run_id(accepted_run_id):
                raise SafetyError("supervisor acceptance history has an invalid run_id")
            if accepted_run_id in accepted_run_ids:
                raise SafetyError("supervisor acceptance history reuses a run_id")
            accepted_run_ids.add(accepted_run_id)
            _require_digest(item.get("result_sha256"), "supervisor accepted result_sha256")
        active = state.get("active")
        if active is not None:
            # In state schema v1, next_sequence denotes the current unaccepted
            # entry.  It advances only after that entry is inspect-accepted.
            if not isinstance(active, dict) or active.get("sequence") != nxt:
                raise SafetyError("supervisor active binding is not the next sequence")
            expected_predecessors = list(range(1, active["sequence"]))
            if accepted_sequences != expected_predecessors:
                raise SafetyError("supervisor active binding has an unaccepted predecessor")
            if not isinstance(active.get("brief"), str):
                raise SafetyError("supervisor active binding has no brief")
            if not isinstance(active.get("brief_sha256"), str):
                raise SafetyError("supervisor active binding has no brief_sha256")
            if active.get("run_id") is None:
                # A process that died between intent persistence and host start
                # must not be guessed or retried automatically.
                raise SafetyError("supervisor has an unresolved launch intent")
            if not _valid_run_id(active.get("run_id")):
                raise SafetyError("supervisor active run_id is not canonical UUIDv7")
            if active["run_id"] in accepted_run_ids:
                raise SafetyError("supervisor active binding reuses an accepted run_id")
        if len(acceptances) != last:
            raise SafetyError("supervisor acceptance history is not contiguous")
        return state

    def _load_state(self, plan_digest: str, count: int) -> dict[str, Any]:
        if self.state_path.exists():
            value, _ = _read_json(self.state_path, "supervisor state")
            return self._validate_state(value, plan_digest, count)
        state = self._initial_state(plan_digest)
        _atomic_json(self.state_path, state)
        return state

    def _save_state(self, state: dict[str, Any]) -> None:
        state["updated_at"] = _now()
        _atomic_json(self.state_path, state)

    def _invoke(self, args: list[str], timeout: float = 90.0) -> Invocation:
        command = [str(self.quinte_bin), "--home", str(self.home), *args, "--json"]
        environment = os.environ.copy()
        environment["QUINTE_HOME"] = str(self.home)
        return self.runner(command, environment, timeout)

    def _receipt(
        self,
        invocation: Invocation,
        operation: str,
        *,
        run_id: str | None = None,
        allow_nonzero: bool = False,
    ) -> dict[str, Any]:
        if not invocation.output_ok or invocation.payload is None:
            raise SafetyError(
                f"host {operation} produced malformed output: {json.dumps(invocation.record(), ensure_ascii=False)}"
            )
        if not allow_nonzero and invocation.returncode != 0:
            raise SafetyError(
                f"host {operation} exited {invocation.returncode}: {json.dumps(invocation.record(), ensure_ascii=False)}"
            )
        envelope = invocation.payload
        if envelope.get("ok") is not True:
            raise SafetyError(f"host {operation} envelope is not ok")
        data = envelope.get("data")
        if not isinstance(data, dict):
            raise SafetyError(f"host {operation} has no data object")
        if data.get("host_receipt_version") != HOST_RECEIPT_VERSION:
            raise SafetyError(f"host {operation} receipt version is not {HOST_RECEIPT_VERSION}")
        if data.get("operation") != operation:
            raise SafetyError(f"expected host {operation} receipt, got {data.get('operation')!r}")
        if data.get("state_root") != str(self.home):
            raise SafetyError(f"host {operation} receipt state_root does not match QUINTE_HOME")
        if run_id is not None and data.get("run_id") != run_id:
            raise SafetyError(f"host {operation} receipt run_id does not match active run")
        invocation_id = data.get("invocation_id")
        if not _valid_run_id(invocation_id):
            raise SafetyError(f"host {operation} receipt invocation_id is not canonical UUIDv7")
        receipt_path = data.get("receipt_path")
        if not isinstance(receipt_path, str):
            raise SafetyError(f"host {operation} receipt has no receipt_path")
        receipt = _absolute(receipt_path, f"host {operation} receipt_path")
        expected_receipt = (self.home / "host" / "receipts" / f"{invocation_id}.json").resolve()
        if receipt != expected_receipt:
            raise SafetyError(f"host {operation} receipt_path is not bound to invocation_id")
        manifest = data.get("manifest")
        if isinstance(manifest, dict):
            runtime = _require_digest(manifest.get("runtime_sha256"), f"host {operation} runtime_sha256")
            if runtime != self.runtime_sha256:
                raise SafetyError(f"host {operation} reports a different runtime digest")
        return data

    @staticmethod
    def _active_ids(data: dict[str, Any], operation: str) -> list[str]:
        state = data.get("state")
        if not isinstance(state, dict) or not isinstance(state.get("active_run_ids"), list):
            raise SafetyError(f"host {operation} receipt has malformed active_run_ids")
        ids = state["active_run_ids"]
        if any(not _valid_run_id(value) for value in ids) or len(set(ids)) != len(ids):
            raise SafetyError(f"host {operation} receipt has invalid active run ids")
        return ids

    def _check_manifest(self, data: dict[str, Any], entry: dict[str, Any], operation: str) -> dict[str, Any]:
        manifest = data.get("manifest")
        if not isinstance(manifest, dict):
            raise SafetyError(f"host {operation} receipt has no manifest")
        status = manifest.get("status")
        if status not in ALL_STATUSES:
            raise SafetyError(f"host {operation} reports unknown status {status!r}")
        if manifest.get("brief_sha256") != entry["brief_sha256"]:
            raise SafetyError(f"host {operation} brief binding does not match plan entry")
        return manifest

    def _preflight(self) -> tuple[Invocation, dict[str, Any]]:
        invocation = self._invoke(["host", "preflight"])
        data = self._receipt(invocation, "preflight", allow_nonzero=True)
        ids = self._active_ids(data, "preflight")
        state = data.get("state")
        if state.get("code") != "ready" or ids:
            raise SafetyError(
                f"preflight is not launch-safe: code={state.get('code')!r}, active_run_ids={ids!r}"
            )
        if invocation.returncode != 0:
            raise SafetyError("launch-safe preflight returned a nonzero process status")
        preflight = data.get("preflight")
        if not isinstance(preflight, dict) or preflight.get("ok") is not True:
            raise SafetyError("preflight receipt is not ok")
        return invocation, data

    def _status(
        self, active: dict[str, Any], entry: dict[str, Any]
    ) -> tuple[Invocation, dict[str, Any], str]:
        run_id = active["run_id"]
        invocation = self._invoke(["host", "status", run_id])
        data = self._receipt(invocation, "status", run_id=run_id)
        manifest = self._check_manifest(data, entry, "status")
        status = manifest["status"]
        ids = self._active_ids(data, "status")
        if status in ACTIVE_STATUSES and ids != [run_id]:
            raise SafetyError(f"active status is ambiguous: {ids!r}")
        if status in TERMINAL_STATUSES and ids:
            raise SafetyError(f"terminal status still reports active runs: {ids!r}")
        return invocation, data, status

    def _inspect_terminal(
        self, run_id: str, entry: dict[str, Any], expected_status: str
    ) -> tuple[Invocation, dict[str, Any]]:
        invocation = self._invoke(["host", "inspect", run_id])
        # Inspect returns status_code 1 for degraded/failed, but its JSON
        # receipt remains useful and must not be discarded.
        data = self._receipt(invocation, "inspect", run_id=run_id, allow_nonzero=True)
        manifest = self._check_manifest(data, entry, "inspect")
        if manifest["status"] != expected_status:
            raise SafetyError("status changed between host status and host inspect")
        if expected_status == "completed" and invocation.returncode != 0:
            raise SafetyError("completed host inspect returned a nonzero process status")
        return invocation, data

    def _verify_completed_result(
        self, data: dict[str, Any], entry: dict[str, Any]
    ) -> dict[str, Any]:
        result = data.get("result")
        manifest = data.get("manifest")
        if not isinstance(result, dict) or not isinstance(manifest, dict):
            raise SafetyError("completed inspect has no result binding")
        if result.get("verified") is not True or result.get("actionable") is not True:
            raise SafetyError("terminal result is not verified and actionable")
        result_digest = _require_digest(result.get("sha256"), "terminal result sha256")
        if result_digest != manifest.get("result_sha256"):
            raise SafetyError("terminal result digest does not match manifest")
        path = _absolute(result.get("path"), "terminal result path", must_exist=True)
        runs_root = (self.home / "runs").resolve()
        try:
            path.relative_to(runs_root)
        except ValueError as exc:
            raise SafetyError("terminal result path escapes QUINTE_HOME/runs") from exc
        expected_path = (runs_root / data["run_id"] / "result.json").resolve()
        if path != expected_path:
            raise SafetyError("terminal result path is not the active run's result.json")
        if not path.is_file() or _digest_file(path) != result_digest:
            raise SafetyError("terminal result bytes do not match inspect digest")
        if manifest.get("brief_sha256") != entry["brief_sha256"]:
            raise SafetyError("terminal result brief binding does not match plan entry")
        contract_version = result.get("contract_version")
        if not isinstance(contract_version, str) or not contract_version:
            raise SafetyError("terminal result has no contract_version")
        return {
            "run_id": data["run_id"],
            "sequence": entry["sequence"],
            "result_sha256": result_digest,
            "contract_version": contract_version,
            "inspect_invocation_id": data.get("invocation_id"),
            "accepted_at": _now(),
        }

    def _run_locked(self) -> dict[str, Any]:
        if self.halted_path.exists():
            raise HaltedError(f"campaign is halted; inspect {self.halted_path}")
        self._guard_binary()
        _, plan_digest, entries = self._load_plan()
        state = self._load_state(plan_digest, len(entries))

        if state.get("active") is not None:
            active = state["active"]
            entry = entries[state["next_sequence"] - 1]
            if active.get("brief") != entry["brief"]:
                raise SafetyError("active brief does not match the next plan entry")
            if active.get("brief_sha256") != entry["brief_sha256"]:
                raise SafetyError("active brief digest does not match the next plan entry")
            status_invocation, status_data, status = self._status(active, entry)
            state["last_observation"] = {
                "kind": "status",
                "observed_at": _now(),
                "sequence": entry["sequence"],
                "run_id": active["run_id"],
                "status": status,
                "receipt": status_invocation.record(),
            }
            self._save_state(state)
            if status in ACTIVE_STATUSES:
                return {"status": "active", "sequence": entry["sequence"], "run_id": active["run_id"], "state": state}

            inspect_invocation, inspect_data = self._inspect_terminal(active["run_id"], entry, status)
            state["last_observation"]["inspect"] = inspect_invocation.record()
            # Persist the terminal inspect receipt before validating result
            # bytes.  If validation fails, HALTED still contains the exact
            # evidence needed for manual review and no later launch is possible.
            self._save_state(state)
            if status != "completed":
                self._halt(
                    "terminal_status_not_admissible",
                    f"sequence {entry['sequence']} reached {status}; manual review required",
                    state=state,
                )
            acceptance = self._verify_completed_result(inspect_data, entry)
            state["acceptances"].append(acceptance)
            state["last_accepted_sequence"] = entry["sequence"]
            state["next_sequence"] = entry["sequence"] + 1
            state["active"] = None
            state["last_observation"]["accepted"] = acceptance
            self._save_state(state)
            if state["next_sequence"] > len(entries):
                return {"status": "done", "state": state}

        if state["next_sequence"] > len(entries):
            return {"status": "done", "state": state}

        entry = entries[state["next_sequence"] - 1]
        preflight_invocation, preflight_data = self._preflight()
        state["last_observation"] = {
            "kind": "preflight",
            "observed_at": _now(),
            "sequence": entry["sequence"],
            "receipt": preflight_invocation.record(),
        }
        self._save_state(state)
        command = [str(self.quinte_bin), "--home", str(self.home), "host", "start", "--brief", entry["brief"], "--json"]
        if not self.execute:
            return {
                "status": "dry-run",
                "sequence": entry["sequence"],
                "brief": entry["brief"],
                "command": command,
                "state": state,
            }

        # Persist intent before dispatch.  If the supervisor dies here, the
        # next invocation sees run_id=null and fails closed rather than retrying.
        state["active"] = {
            "sequence": entry["sequence"],
            "brief": entry["brief"],
            "brief_sha256": entry["brief_sha256"],
            "run_id": None,
            "launch_started_at": _now(),
        }
        self._save_state(state)
        start_invocation = self._invoke(["host", "start", "--brief", entry["brief"]], timeout=120)
        state["last_observation"] = {
            "kind": "start",
            "observed_at": _now(),
            "sequence": entry["sequence"],
            "receipt": start_invocation.record(),
        }
        self._save_state(state)
        start_data = self._receipt(start_invocation, "start")
        run_id = start_data.get("run_id")
        if not _valid_run_id(run_id):
            raise SafetyError("host start did not return a canonical run_id")
        accepted_run_ids = {
            acceptance.get("run_id") for acceptance in state["acceptances"]
        }
        if run_id in accepted_run_ids:
            raise SafetyError("host start reused an accepted campaign run_id")
        start_state = start_data.get("state")
        if not isinstance(start_state, dict) or start_state.get("code") != "started":
            raise SafetyError("host start did not return state.code=started")
        if self._active_ids(start_data, "start") != [run_id]:
            raise SafetyError("host start active run set is not exactly the new run")
        self._check_manifest(start_data, entry, "start")
        state["active"]["run_id"] = run_id
        state["last_observation"]["run_id"] = run_id
        self._save_state(state)
        return {"status": "launched", "sequence": entry["sequence"], "run_id": run_id, "state": state}

    def run(self) -> dict[str, Any]:
        try:
            with _exclusive_lock(self.lock_path):
                try:
                    return self._run_locked()
                except HaltedError:
                    raise
                except SafetyError as exc:
                    # Keep the last valid state, if available, in the sentinel.
                    state = None
                    if self.state_path.is_file():
                        with contextlib.suppress(Exception):
                            state, _ = _read_json(self.state_path, "supervisor state")
                    self._halt(type(exc).__name__, str(exc), state=state if isinstance(state, dict) else None)
        except LockBusyError:
            raise


def _run_command(command: list[str], environment: dict[str, str], timeout: float) -> Invocation:
    try:
        process = subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=environment,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode() if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        payload, error = _parse_payload(stdout)
        return Invocation(tuple(command), None, stdout, stderr, payload, error, True, f"timed out after {timeout:g}s")
    except OSError as exc:
        return Invocation(tuple(command), None, "", "", None, None, False, str(exc))
    payload, error = _parse_payload(process.stdout)
    return Invocation(tuple(command), process.returncode, process.stdout, process.stderr, payload, error)


def _parse_payload(stdout: str) -> tuple[dict[str, Any] | None, str | None]:
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as exc:
        return None, str(exc)
    if not isinstance(value, dict):
        return None, "stdout JSON is not an object"
    return value, None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True, help="absolute immutable plan JSON")
    parser.add_argument("--state", required=True, help="absolute supervisor state JSON")
    parser.add_argument("--home", required=True, help="absolute QUINTE_HOME")
    parser.add_argument("--quinte-bin", required=True, help="absolute pinned quinte executable")
    parser.add_argument("--runtime-sha256", required=True, help="sha256:<64 lowercase hex>")
    parser.add_argument("--execute", action="store_true", help="launch exactly one next sequence")
    parser.add_argument("--json", action="store_true", help="emit the result as JSON")
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        supervisor = Supervisor(
            plan=Path(args.plan),
            state=Path(args.state),
            home=Path(args.home),
            quinte_bin=Path(args.quinte_bin),
            runtime_sha256=args.runtime_sha256,
            execute=args.execute,
        )
        result = supervisor.run()
    except LockBusyError as exc:
        print(str(exc), file=sys.stderr)
        return 4
    except HaltedError as exc:
        print(str(exc), file=sys.stderr)
        return 3
    except SupervisorError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f"contest supervisor: {result.get('status')}")
        if result.get("run_id"):
            print(f"run_id: {result['run_id']}")
        if result.get("sequence"):
            print(f"sequence: {result['sequence']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
