#!/usr/bin/env python3
"""Offline contract tests for scripts/contest_supervisor.py.

These tests inject host receipts and never invoke a real QUINTE worker or
provider.  They exercise the safety decisions that are independent of a live
campaign: pinning, one-shot observation, terminal acceptance, sequencing, and
fail-stop persistence.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "scripts" / "contest_supervisor.py"
SPEC = importlib.util.spec_from_file_location("contest_supervisor", SCRIPT)
assert SPEC and SPEC.loader
supervisor = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = supervisor
SPEC.loader.exec_module(supervisor)


RUN_1 = "019fd896-7769-7c62-a3c3-e4f34fbc09f2"
RUN_2 = "019fd896-7769-7c62-a3c3-e4f34fbc09f4"
INVOCATION = "019fd896-7769-7c62-a3c3-e4f34fbc09f3"


def digest(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def envelope(data: dict, *, ok: bool = True, returncode: int = 0) -> supervisor.Invocation:
    data = dict(data)
    data.setdefault("invocation_id", INVOCATION)
    state_root = data.get("state_root")
    if isinstance(state_root, str):
        data.setdefault(
            "receipt_path",
            str(Path(state_root) / "host" / "receipts" / f"{INVOCATION}.json"),
        )
    payload = {"cli_envelope_version": "1.0", "ok": ok, "data": data}
    return supervisor.Invocation(
        command=("quinte",),
        returncode=returncode,
        stdout=json.dumps(payload),
        stderr="",
        payload=payload,
    )


class SupervisorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.home = self.root / "quinte-home"
        self.home.mkdir()
        self.binary = self.root / "quinte"
        self.binary.write_bytes(b"pinned-quinte-binary-v1\n")
        self.binary.chmod(self.binary.stat().st_mode | stat.S_IXUSR)
        self.runtime = digest(self.binary.read_bytes())
        self.briefs = self.root / "briefs"
        self.briefs.mkdir()
        self.brief_paths = []
        for sequence in (1, 2):
            path = self.briefs / f"brief-{sequence}.json"
            path.write_text(
                json.dumps(
                    {
                        "brief_version": "1.1",
                        "question": f"review {sequence}",
                        "context": None,
                        "evidence_roots": [],
                        "snapshot_ignore": [],
                        "attachments": [],
                        "action_scope": "decision support",
                        "affected_paths": [],
                        "action_binding_sha256": None,
                    }
                ),
                encoding="utf-8",
            )
            self.brief_paths.append(path)
        self.plan = self.root / "plan.json"
        entries = [
            {
                "sequence": index,
                "brief": str(path),
                "brief_sha256": supervisor._canonical_brief_digest(path),
            }
            for index, path in enumerate(self.brief_paths, 1)
        ]
        self.plan.write_text(
            json.dumps(
                {
                    "schema": supervisor.PLAN_SCHEMA,
                    "state_root": str(self.home),
                    "runtime_sha256": self.runtime,
                    "entries": entries,
                }
            ),
            encoding="utf-8",
        )
        self.state = self.root / "supervisor-state.json"

    def tearDown(self) -> None:
        self.temp.cleanup()

    def make(self, responses, *, execute=False):
        queue = list(responses)
        calls = []

        def runner(command, environment, timeout):
            calls.append((command, environment, timeout))
            if not queue:
                raise AssertionError(f"unexpected command: {command!r}")
            return queue.pop(0)

        instance = supervisor.Supervisor(
            plan=self.plan,
            state=self.state,
            home=self.home,
            quinte_bin=self.binary,
            runtime_sha256=self.runtime,
            execute=execute,
            runner=runner,
        )
        return instance, queue, calls

    def preflight(self):
        return envelope(
            {
                "host_receipt_version": "1.0",
                "operation": "preflight",
                "state_root": str(self.home),
                "state": {"code": "ready", "active_run_ids": []},
                "preflight": {"ok": True},
            }
        )

    def manifest(self, status, brief, *, result_sha=None):
        return {
            "status": status,
            "brief_sha256": supervisor._canonical_brief_digest(brief),
            "runtime_sha256": self.runtime,
            "result_sha256": result_sha,
        }

    def start(self, run_id, brief):
        return envelope(
            {
                "host_receipt_version": "1.0",
                "operation": "start",
                "state_root": str(self.home),
                "run_id": run_id,
                "state": {"code": "started", "active_run_ids": [run_id]},
                "manifest": self.manifest("queued", brief),
            }
        )

    def status(self, run_id, brief, status, *, active=True, returncode=0):
        return envelope(
            {
                "host_receipt_version": "1.0",
                "operation": "status",
                "state_root": str(self.home),
                "run_id": run_id,
                "state": {"code": "observed" if active else "terminal", "active_run_ids": [run_id] if active else []},
                "manifest": self.manifest(status, brief),
            },
            returncode=returncode,
        )

    def inspect(self, run_id, brief, *, actionable=True, status="completed"):
        result_file = self.home / "runs" / run_id / "result.json"
        result_file.parent.mkdir(parents=True, exist_ok=True)
        result_file.write_bytes(b"immutable result\n")
        result_sha = digest(result_file.read_bytes())
        return envelope(
            {
                "host_receipt_version": "1.0",
                "operation": "inspect",
                "state_root": str(self.home),
                "run_id": run_id,
                "state": {"code": "terminal", "active_run_ids": []},
                "manifest": self.manifest(status, brief, result_sha=result_sha),
                "result": {
                    "verified": True,
                    "actionable": actionable,
                    "contract_version": "2.1",
                    "sha256": result_sha,
                    "path": str(result_file),
                },
            },
            returncode=0 if status == "completed" else 1,
        )

    def launch_first(self):
        first, _, _ = self.make(
            [self.preflight(), self.start(RUN_1, self.brief_paths[0])],
            execute=True,
        )
        first.run()

    def accept_first_responses(self, *, start_response=None):
        responses = [
            self.status(RUN_1, self.brief_paths[0], "completed", active=False),
            self.inspect(RUN_1, self.brief_paths[0]),
            self.preflight(),
        ]
        if start_response is not None:
            responses.append(start_response)
        return responses

    def test_default_is_dry_run_and_pinned(self):
        instance, queue, calls = self.make([self.preflight()])
        result = instance.run()
        self.assertEqual(result["status"], "dry-run")
        self.assertEqual(len(calls), 1)
        self.assertIn("--home", calls[0][0])
        self.assertNotIn("host", calls[0][0][0:2])  # binary is argv[0], no accidental lane call
        self.assertFalse((self.home / "runs").exists())
        self.assertFalse(instance.halted_path.exists())
        self.assertEqual(queue, [])

    def test_execute_starts_one_run_and_persists_intent(self):
        instance, _, calls = self.make([self.preflight(), self.start(RUN_1, self.brief_paths[0])], execute=True)
        result = instance.run()
        self.assertEqual(result["status"], "launched")
        self.assertEqual(result["run_id"], RUN_1)
        self.assertEqual(len(calls), 2)
        state = json.loads(self.state.read_text())
        self.assertEqual(state["active"]["run_id"], RUN_1)
        self.assertEqual(state["next_sequence"], 1)

    def test_terminal_requires_separate_inspect_then_advances(self):
        self.launch_first()
        second, _, calls = self.make(
            [
                self.status(RUN_1, self.brief_paths[0], "completed", active=False),
                self.inspect(RUN_1, self.brief_paths[0]),
                self.preflight(),
            ]
        )
        result = second.run()
        self.assertEqual(result["status"], "dry-run")
        self.assertEqual(result["sequence"], 2)
        self.assertEqual([call[0][3:5] for call in calls], [["host", "status"], ["host", "inspect"], ["host", "preflight"]])
        state = json.loads(self.state.read_text())
        self.assertEqual(state["last_accepted_sequence"], 1)
        self.assertIsNone(state["active"])

    def test_degraded_status_halts_and_writes_sentinel(self):
        self.launch_first()
        instance, _, _ = self.make(
            [
                self.status(RUN_1, self.brief_paths[0], "degraded", active=False),
                self.inspect(RUN_1, self.brief_paths[0], status="degraded"),
            ]
        )
        with self.assertRaises(supervisor.HaltedError):
            instance.run()
        halted = json.loads(instance.halted_path.read_text())
        self.assertEqual(halted["schema"], supervisor.HALTED_SCHEMA)
        self.assertIn("not_admissible", halted["reason"])

    def test_non_actionable_result_halts(self):
        self.launch_first()
        instance, _, _ = self.make(
            [
                self.status(RUN_1, self.brief_paths[0], "completed", active=False),
                self.inspect(RUN_1, self.brief_paths[0], actionable=False),
            ]
        )
        with self.assertRaises(supervisor.HaltedError):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_digest_drift_halts_before_any_cli_call(self):
        self.binary.write_bytes(b"replacement-binary\n")
        instance, _, calls = self.make([])
        with self.assertRaises(supervisor.HaltedError):
            instance.run()
        self.assertEqual(calls, [])
        self.assertTrue(instance.halted_path.exists())

    def test_malformed_status_halts_without_launching_next(self):
        self.launch_first()
        malformed = supervisor.Invocation(
            command=("quinte", "host", "status"),
            returncode=0,
            stdout="not json",
            stderr="",
            payload=None,
            parse_error="invalid JSON",
        )
        instance, _, calls = self.make([malformed])
        with self.assertRaises(supervisor.HaltedError):
            instance.run()
        self.assertEqual(len(calls), 1)
        self.assertTrue(instance.halted_path.exists())

    def test_result_path_must_be_exact_run_result(self):
        self.launch_first()
        inspection = self.inspect(RUN_1, self.brief_paths[0])
        wrong_path = self.home / "runs" / RUN_1 / "other-result.json"
        wrong_path.write_bytes(b"immutable result\n")
        inspection.payload["data"]["result"]["path"] = str(wrong_path)
        instance, _, _ = self.make(
            [
                self.status(RUN_1, self.brief_paths[0], "completed", active=False),
                inspection,
            ]
        )
        with self.assertRaisesRegex(
            supervisor.HaltedError, "active run's result.json"
        ):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_active_run_id_cannot_reuse_an_acceptance(self):
        self.launch_first()
        state = json.loads(self.state.read_text())
        state["acceptances"] = [
            {
                "sequence": 1,
                "run_id": RUN_1,
                "result_sha256": digest(b"result"),
            }
        ]
        state["last_accepted_sequence"] = 1
        state["next_sequence"] = 2
        state["active"] = {
            "sequence": 2,
            "brief": str(self.brief_paths[1]),
            "brief_sha256": supervisor._canonical_brief_digest(self.brief_paths[1]),
            "run_id": RUN_1,
        }
        self.state.write_text(json.dumps(state), encoding="utf-8")
        instance, _, _ = self.make([])
        with self.assertRaisesRegex(supervisor.HaltedError, "reuses an accepted"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_result_bytes_must_match_inspect_digest(self):
        self.launch_first()
        inspection = self.inspect(RUN_1, self.brief_paths[0])
        result_file = self.home / "runs" / RUN_1 / "result.json"
        result_file.write_bytes(b"tampered result\n")
        instance, _, _ = self.make(
            [
                self.status(RUN_1, self.brief_paths[0], "completed", active=False),
                inspection,
            ]
        )
        with self.assertRaisesRegex(supervisor.HaltedError, "result bytes"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_duplicate_run_id_in_acceptance_history_halts(self):
        self.launch_first()
        state = json.loads(self.state.read_text())
        result_sha = digest(b"result")
        state["acceptances"] = [
            {"sequence": 1, "run_id": RUN_1, "result_sha256": result_sha},
            {"sequence": 2, "run_id": RUN_1, "result_sha256": result_sha},
        ]
        state["last_accepted_sequence"] = 2
        state["next_sequence"] = 3
        state["active"] = None
        self.state.write_text(json.dumps(state), encoding="utf-8")
        instance, _, _ = self.make([])
        with self.assertRaisesRegex(supervisor.HaltedError, "reuses a run_id"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_host_start_cannot_reuse_accepted_run_id(self):
        self.launch_first()
        instance, _, _ = self.make(
            self.accept_first_responses(
                start_response=self.start(RUN_1, self.brief_paths[1])
            ),
            execute=True,
        )
        with self.assertRaisesRegex(supervisor.HaltedError, "reused an accepted"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_active_binding_requires_current_next_sequence(self):
        self.launch_first()
        state = json.loads(self.state.read_text())
        state["active"]["sequence"] = 2
        self.state.write_text(json.dumps(state), encoding="utf-8")
        instance, _, _ = self.make([])
        with self.assertRaisesRegex(supervisor.HaltedError, "not the next sequence"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_active_binding_requires_all_predecessors_accepted(self):
        self.launch_first()
        state = json.loads(self.state.read_text())
        state["active"]["sequence"] = 2
        state["next_sequence"] = 2
        state["last_accepted_sequence"] = 1
        state["acceptances"] = []
        self.state.write_text(json.dumps(state), encoding="utf-8")
        instance, _, _ = self.make([])
        with self.assertRaisesRegex(supervisor.HaltedError, "unaccepted predecessor"):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_plan_sequence_gap_is_fail_stop(self):
        plan = json.loads(self.plan.read_text())
        plan["entries"][1]["sequence"] = 3
        self.plan.write_text(json.dumps(plan), encoding="utf-8")
        instance, _, _ = self.make([])
        with self.assertRaises(supervisor.HaltedError):
            instance.run()
        self.assertTrue(instance.halted_path.exists())

    def test_lock_is_nonblocking(self):
        instance, _, _ = self.make([self.preflight()])
        with supervisor._exclusive_lock(instance.lock_path):
            with self.assertRaises(supervisor.LockBusyError):
                instance.run()


if __name__ == "__main__":
    unittest.main()
