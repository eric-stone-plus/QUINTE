#!/usr/bin/env python3
"""Standalone regression tests for scripts/quinte-audit (stdlib only)."""

import hashlib
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("quinte-audit")
VALID_PASS = {
    "verdict": "PASS",
    "confidence": 0.9,
    "findings": [],
    "summary": "Audit completed.",
}
VALID_FAIL = {
    "verdict": "FAIL",
    "confidence": 0.8,
    "findings": [
        {"kind": "evidence", "severity": "HIGH", "detail": "Evidence is insufficient."}
    ],
    "summary": "The verdict is not supported.",
}


FAKE_CLI = r'''#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path

Path(os.environ["FAKE_ARGS_DIR"]).joinpath(Path(sys.argv[0]).name + ".json").write_text(
    json.dumps(sys.argv[1:])
)
print(os.environ.get("FAKE_AUDIT_OUTPUT", "{}"))
'''


class QuinteAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.home = self.root / "custom-home"
        self.run_id = "019f-test-run"
        self.run_dir = self.home / "runs" / self.run_id
        (self.run_dir / "r3").mkdir(parents=True)
        (self.run_dir / "result.json").write_text(
            json.dumps(
                {
                    "run_id": self.run_id,
                    "status": "completed",
                    "question": "Review this.",
                    "summary": "Summary.",
                    "recommendation": "Recommendation.",
                    "residuals": [],
                }
            )
        )
        self.bin_dir = self.root / "bin"
        self.bin_dir.mkdir()
        for name in ("omp",):
            path = self.bin_dir / name
            path.write_text(FAKE_CLI)
            path.chmod(path.stat().st_mode | stat.S_IXUSR)
        self.args_dir = self.root / "args"
        self.args_dir.mkdir()
        self.environment = os.environ.copy()
        self.environment.update(
            {
                "PATH": f"{self.bin_dir}{os.pathsep}{self.environment.get('PATH', '')}",
                "FAKE_ARGS_DIR": str(self.args_dir),
                "PYTHONDONTWRITEBYTECODE": "1",
            }
        )
        self.environment.pop("QUINTE_AUDITOR", None)
        self.environment.pop("QUINTE_AUDIT_MODEL", None)
        self.environment.pop("QUINTE_HOME", None)

    def tearDown(self):
        self.temporary.cleanup()

    def run_audit(self, *arguments, output=VALID_PASS, environment=None):
        env = dict(environment or self.environment)
        env["FAKE_AUDIT_OUTPUT"] = output if isinstance(output, str) else json.dumps(output)
        return subprocess.run(
            [str(SCRIPT), *arguments], capture_output=True, text=True, env=env, timeout=10
        )

    def pointer(self, name="external-audit.json"):
        return json.loads((self.run_dir / "r3" / name).read_text())

    def history_paths(self):
        return sorted((self.run_dir / "r3").glob("external-audit-standard-*.json"))

    def test_custom_home_flag_resolves_non_default_run(self):
        process = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", "--json",
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(json.loads(process.stdout)["verdict"], "PASS")

    def test_quinte_home_environment_is_used(self):
        environment = dict(self.environment, QUINTE_HOME=str(self.home))
        process = self.run_audit(
            self.run_id, "--auditor", "omp", "--model", "deepseek/deepseek-v4-pro",
            environment=environment,
        )
        self.assertEqual(process.returncode, 0, process.stderr)

    def test_omp_requires_explicit_model(self):
        process = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp"
        )
        self.assertEqual(process.returncode, 2)
        self.assertIn("requires an explicit --model", process.stderr)
        self.assertFalse((self.run_dir / "r3" / "external-audit.json").exists())

    def test_omp_is_tool_free_and_has_fixed_system_prompt(self):
        process = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro",
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        arguments = json.loads((self.args_dir / "omp.json").read_text())
        self.assertIn("--no-tools", arguments)
        self.assertNotIn("--tools", arguments)
        self.assertIn("--system-prompt", arguments)
        system_prompt = arguments[arguments.index("--system-prompt") + 1]
        self.assertIn("isolated post-protocol external auditor", system_prompt)
        self.assertEqual(arguments[arguments.index("--model") + 1], "deepseek/deepseek-v4-pro")

    def test_invalid_output_creates_no_artifact(self):
        for invalid in ("{}", "not json", '{"verdict":"PASS"}'):
            with self.subTest(invalid=invalid):
                process = self.run_audit(
                    self.run_id, "--home", str(self.home), "--auditor", "omp",
                    "--model", "deepseek/deepseek-v4-pro", output=invalid,
                )
                self.assertEqual(process.returncode, 2)
                self.assertFalse((self.run_dir / "r3" / "external-audit.json").exists())
                self.assertEqual(self.history_paths(), [])

    def test_valid_fail_returns_20_and_preserves_verdict(self):
        process = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", "--json", output=VALID_FAIL,
        )
        self.assertEqual(process.returncode, 20, process.stderr)
        pointer = json.loads(process.stdout)
        self.assertEqual(pointer["verdict"], "FAIL")
        self.assertEqual(self.pointer()["verdict"], "FAIL")

    def test_repeated_audits_keep_immutable_history_and_update_pointer(self):
        first = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", output=VALID_PASS,
        )
        self.assertEqual(first.returncode, 0, first.stderr)
        first_path = self.history_paths()[0]
        first_bytes = first_path.read_bytes()
        second = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", output=VALID_FAIL,
        )
        self.assertEqual(second.returncode, 20, second.stderr)
        histories = self.history_paths()
        self.assertEqual(len(histories), 2)
        self.assertEqual(first_path.read_bytes(), first_bytes)
        pointer = self.pointer()
        latest = self.run_dir / pointer["latest_artifact_ref"]
        self.assertEqual(pointer["verdict"], "FAIL")
        self.assertEqual(pointer["sha256"], f"sha256:{hashlib.sha256(latest.read_bytes()).hexdigest()}")
        self.assertFalse(list((self.run_dir / "r3").glob(".external-audit.json.*")))

    def test_default_omp_still_requires_explicit_model(self):
        process = self.run_audit(self.run_id, "--home", str(self.home), "--json")
        self.assertEqual(process.returncode, 2)
        self.assertIn("OMP requires an explicit --model", process.stderr)

    def test_invalid_auditor_environment_is_rejected(self):
        environment = dict(self.environment, QUINTE_AUDITOR="not-an-auditor")
        process = self.run_audit(
            self.run_id, "--home", str(self.home), environment=environment
        )
        self.assertEqual(process.returncode, 2)
        self.assertIn("invalid choice", process.stderr)

    def test_paradigm_pointer_does_not_replace_standard_pointer(self):
        standard = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", output=VALID_PASS,
        )
        self.assertEqual(standard.returncode, 0, standard.stderr)
        standard_bytes = (self.run_dir / "r3" / "external-audit.json").read_bytes()
        paradigm_output = {
            "verdict": "PASS_WITH_NOTES",
            "confidence": 0.7,
            "findings": [
                {"kind": "framework", "severity": "MEDIUM", "detail": "Shared framing remains."}
            ],
            "summary": "The framework has a shared blind spot.",
        }
        paradigm = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", "--mode", "paradigm",
            output=paradigm_output,
        )
        self.assertEqual(paradigm.returncode, 10, paradigm.stderr)
        self.assertEqual((self.run_dir / "r3" / "external-audit.json").read_bytes(), standard_bytes)
        self.assertEqual(self.pointer("external-audit-paradigm.json")["verdict"], "PASS_WITH_NOTES")

    def test_legacy_canonical_full_report_is_archived_before_pointer_migration(self):
        legacy_bytes = b'{"audit_version":"1.0","audit":{"verdict":"PASS"}}\n'
        canonical = self.run_dir / "r3" / "external-audit.json"
        canonical.write_bytes(legacy_bytes)
        process = self.run_audit(
            self.run_id, "--home", str(self.home), "--auditor", "omp",
            "--model", "deepseek/deepseek-v4-pro", output=VALID_PASS,
        )
        self.assertEqual(process.returncode, 0, process.stderr)
        digest = hashlib.sha256(legacy_bytes).hexdigest()
        archived = self.run_dir / "r3" / f"external-audit-standard-legacy-{digest[:16]}.json"
        self.assertEqual(archived.read_bytes(), legacy_bytes)
        self.assertEqual(self.pointer()["external_audit_pointer_version"], "1.0")


if __name__ == "__main__":
    unittest.main(verbosity=2)
