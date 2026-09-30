"""Real process regressions: bound the tests of the timeout mechanism too."""

import contextlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

import local_ci


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def run_gate(self, steps, python=None, setup=""):
        config = self.root / ".github/local-ci.json"
        config.parent.mkdir(exist_ok=True)
        config.write_text(json.dumps({"steps": steps}), encoding="utf-8")
        code = (
            "import pathlib, sys; sys.path.insert(0, sys.argv[1])\n"
            "import local_ci\n" + setup + "\n"
            "local_ci.ROOT = pathlib.Path(sys.argv[2])\n"
            "raise SystemExit(local_ci.main([]))"
        )
        # A broken cleanup must not hang this test's communicate() on Windows
        # when an escaped child retains a captured pipe.
        with (
            (self.root / "stdout").open("w+", encoding="utf-8") as stdout,
            (self.root / "stderr").open("w+", encoding="utf-8") as stderr,
        ):
            result = subprocess.run(
                [
                    python or sys.executable,
                    "-B",
                    "-c",
                    code,
                    str(Path(local_ci.__file__).parent),
                    str(self.root),
                ],
                stdout=stdout,
                stderr=stderr,
                timeout=20,
            )
            stdout.seek(0)
            stderr.seek(0)
            return subprocess.CompletedProcess(
                result.args,
                result.returncode,
                stdout.read(),
                stderr.read(),
            )

    def step(self, code, timeout=3):
        return {
            "name": "fixture tests",
            "timeout_seconds": timeout,
            "argv": ["{python}", "-u", "-c", code],
        }

    def test_nonzero_reports_reason_and_does_not_run_next_step(self):
        result = self.run_gate(
            [
                self.step(
                    "import sys; print('assertion reason', file=sys.stderr); sys.exit(7)"
                ),
                self.step("from pathlib import Path; Path('should-not-run').touch()"),
            ]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("assertion reason", result.stderr)
        self.assertIn("fixture tests failed with exit code 7", result.stderr)
        self.assertFalse((self.root / "should-not-run").exists())

    def process_running(self, pid):
        if os.name == "nt":
            import ctypes
            from ctypes import wintypes

            api = ctypes.WinDLL("kernel32", use_last_error=True)
            api.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
            api.OpenProcess.restype = wintypes.HANDLE
            api.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
            api.WaitForSingleObject.restype = wintypes.DWORD
            api.CloseHandle.argtypes = [wintypes.HANDLE]
            api.CloseHandle.restype = wintypes.BOOL
            handle = api.OpenProcess(0x00100000, False, pid)
            if not handle:
                return False
            try:
                return api.WaitForSingleObject(handle, 0) == 258
            finally:
                api.CloseHandle(handle)
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        # Orphans may remain as dead zombies until the host's init reaps them.
        stat = Path(f"/proc/{pid}/stat")
        try:
            if stat.is_file() and stat.read_text().rsplit(")", 1)[1].split()[0] == "Z":
                return False
        except FileNotFoundError:
            return False
        return True

    def cleanup_fixture_children(self):
        for filename in ("child.pid", "parent.pid"):
            path = self.root / filename
            if path.exists():
                pid = int(path.read_text())
                if self.process_running(pid):
                    os.kill(pid, signal.SIGTERM if os.name == "nt" else signal.SIGKILL)

    def assert_children_stopped(self):
        for filename in ("child.pid", "parent.pid"):
            path = self.root / filename
            self.assertTrue(path.exists(), f"{filename} was not started")
            pid = int(path.read_text())
            deadline = time.monotonic() + 3
            while self.process_running(pid) and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertFalse(
                self.process_running(pid), f"fixture process {pid} survived"
            )

    def tree_step(self, ending):
        self.addCleanup(self.cleanup_fixture_children)
        child = (
            "import os, signal, time; from pathlib import Path; "
            "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            "Path('child.pid').write_text(str(os.getpid())); time.sleep(120)"
        )
        code = (
            "import os, subprocess, sys, time; from pathlib import Path; "
            "Path('parent.pid').write_text(str(os.getpid())); "
            f"subprocess.Popen([sys.executable, '-u', '-c', {child!r}]); "
            "deadline = time.monotonic() + 2\n"
            "while not Path('child.pid').exists() and time.monotonic() < deadline:\n"
            " time.sleep(0.01)\n"
            "print('child launched', flush=True)\n" + ending
        )
        return self.step(code)

    def test_timeout_terminates_hanging_process_tree_and_prints_deadline(self):
        result = self.run_gate(
            [
                self.tree_step("time.sleep(120)"),
                self.step("from pathlib import Path; Path('should-not-run').touch()"),
            ]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("child launched", result.stdout)
        self.assertIn("fixture tests timed out after 3s", result.stderr)
        self.assert_children_stopped()
        self.assertFalse((self.root / "should-not-run").exists())

    def test_exited_parent_does_not_leave_children_holding_output_open(self):
        for exit_code in (0, 7):
            with self.subTest(exit_code=exit_code):
                result = self.run_gate([self.tree_step(f"sys.exit({exit_code})")])
                self.assertEqual(result.returncode, 0 if exit_code == 0 else 1)
                self.assert_children_stopped()
                for filename in ("child.pid", "parent.pid"):
                    (self.root / filename).unlink()

    def test_cleanup_wait_is_bounded(self):
        process = Mock()
        process.wait.side_effect = subprocess.TimeoutExpired("fixture", 10)
        job = Mock()
        with self.assertRaises(subprocess.TimeoutExpired):
            local_ci.stop_process(process, job)
        process.wait.assert_called_once_with(timeout=local_ci.CLEANUP_TIMEOUT)
        job.terminate.assert_called_once()
        job.close.assert_called_once()

    def test_cleanup_failure_preserves_original_timeout(self):
        process = Mock()
        process.wait.side_effect = subprocess.TimeoutExpired("fixture", 3)
        output = io.StringIO()
        with (
            patch("local_ci.subprocess.Popen", return_value=process),
            patch("local_ci_windows.WindowsJob"),
            patch("local_ci.stop_process", side_effect=OSError("cleanup reason")),
            contextlib.redirect_stderr(output),
        ):
            with self.assertRaisesRegex(
                RuntimeError, "timed out after 3s.*cleanup reason"
            ):
                local_ci.run_step(self.root, self.step(""), self.root)
        self.assertIn("fixture tests timed out after 3s", output.getvalue())

    def test_interrupt_terminates_process_and_reports_reason(self):
        process = Mock()
        process.wait.side_effect = KeyboardInterrupt
        with (
            patch("local_ci.subprocess.Popen", return_value=process),
            patch("local_ci_windows.WindowsJob"),
            patch("local_ci.stop_process") as stop,
        ):
            with self.assertRaisesRegex(RuntimeError, "fixture tests interrupted"):
                local_ci.run_step(self.root, self.step(""), self.root)
        stop.assert_called_once()

    def test_invalid_timeout_cannot_disable_deadline(self):
        for timeout in (None, 0, -1, True, "3", float("inf"), float("nan")):
            with (
                self.subTest(timeout=timeout),
                patch("local_ci.subprocess.Popen") as start,
            ):
                with self.assertRaisesRegex(RuntimeError, "positive finite timeout"):
                    local_ci.run_step(self.root, self.step("", timeout), self.root)
                start.assert_not_called()

    @unittest.skipUnless(os.name == "nt", "Windows job assignment")
    def test_job_assignment_failure_prevents_command_from_starting(self):
        with patch(
            "local_ci_windows.WindowsJob.assign",
            side_effect=OSError("assignment denied"),
        ):
            with self.assertRaisesRegex(RuntimeError, "assignment denied"):
                local_ci.run_step(
                    self.root,
                    self.step(
                        "from pathlib import Path; Path('should-not-run').touch()"
                    ),
                    self.root,
                )
        self.assertFalse((self.root / "should-not-run").exists())

    @unittest.skipUnless(os.name == "nt", "Windows venv redirector")
    def test_venv_redirector_cannot_escape_delayed_job_assignment(self):
        venv = self.root / "venv"
        subprocess.run(
            [sys.executable, "-m", "venv", "--without-pip", str(venv)],
            check=True,
            capture_output=True,
            timeout=30,
        )
        setup = (
            "import local_ci_windows, time\n"
            "original_assign = local_ci_windows.WindowsJob.assign\n"
            "def delayed_assign(self, pid):\n"
            " time.sleep(0.5)\n"
            " original_assign(self, pid)\n"
            "local_ci_windows.WindowsJob.assign = delayed_assign\n"
        )
        for ending in ("time.sleep(120)", "sys.exit(7)", "sys.exit(0)"):
            with self.subTest(ending=ending):
                result = self.run_gate(
                    [self.tree_step(ending)],
                    python=str(venv / "Scripts/python.exe"),
                    setup=setup,
                )
                self.assertEqual(result.returncode, 0 if ending == "sys.exit(0)" else 1)
                if ending == "time.sleep(120)":
                    self.assertIn("timed out after 3s", result.stderr)
                self.assert_children_stopped()
                for filename in ("child.pid", "parent.pid"):
                    (self.root / filename).unlink()

    def test_diagnostic_write_error_still_terminates_actual_children(self):
        setup = (
            "class BrokenStderr:\n"
            " def write(self, text):\n"
            "  raise OSError('diagnostic destination full')\n"
            " def flush(self):\n"
            "  pass\n"
            "original_stderr = sys.stderr\n"
            "original_step = local_ci.run_step\n"
            "def step_with_broken_stderr(*args):\n"
            " try:\n"
            "  sys.stderr = BrokenStderr()\n"
            "  return original_step(*args)\n"
            " finally:\n"
            "  sys.stderr = original_stderr\n"
            "local_ci.run_step = step_with_broken_stderr\n"
        )
        result = self.run_gate([self.tree_step("time.sleep(120)")], setup=setup)
        self.assertEqual(result.returncode, 1)
        self.assertIn("timed out after 3s", result.stderr)
        self.assertIn("diagnostic destination full", result.stderr)
        self.assert_children_stopped()


if __name__ == "__main__":
    unittest.main()
