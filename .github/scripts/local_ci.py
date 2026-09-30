#!/usr/bin/env python3
"""Run the repository's local validation gate and install its Git hook."""

import argparse
import fnmatch
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
HOOKS = ".github/hooks"
GATE = Path(__file__).resolve().parent
CONTROL_TIMEOUT = 30
CLEANUP_TIMEOUT = 10


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(
        ["git", "-C", str(root), *args],
        text=True,
        timeout=CONTROL_TIMEOUT,
    ).strip()


def installed_gate(root: Path) -> Path:
    common = Path(git(root, "rev-parse", "--git-common-dir"))
    return (root / common).resolve() / "local-ci"


def install(root: Path) -> None:
    destination = installed_gate(root)
    hooks = destination / "hooks"
    current = subprocess.run(
        ["git", "-C", str(root), "config", "--get", "core.hooksPath"],
        text=True,
        capture_output=True,
        timeout=CONTROL_TIMEOUT,
    )
    if current.returncode not in (0, 1):
        raise RuntimeError(f"Cannot inspect core.hooksPath: {current.stderr.strip()}")
    if current.stdout.strip() not in ("", HOOKS, str(hooks)):
        raise RuntimeError(
            "An existing core.hooksPath is configured; integrate it explicitly."
        )
    common_hooks = destination.parent / "hooks"
    if not current.stdout.strip() and (common_hooks / "pre-push").exists():
        raise RuntimeError("An existing pre-push hook must be integrated explicitly.")
    scripts = destination / "scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    hooks.mkdir(exist_ok=True)
    for name in (
        "local_ci.py",
        "local_ci_windows.py",
        "test_local_ci.py",
        "test_local_ci_processes.py",
    ):
        shutil.copy2(root / ".github/scripts" / name, scripts / name)
    shutil.copy2(root / ".github/local-ci.json", destination / "local-ci.json")
    shutil.copy2(root / HOOKS / "pre-push", hooks / "pre-push")
    (hooks / "pre-push").chmod(0o755)
    subprocess.run(
        ["git", "-C", str(root), "config", "--local", "core.hooksPath", str(hooks)],
        check=True,
        timeout=CONTROL_TIMEOUT,
    )
    print(
        f"Installed pre-push validation in {hooks} for this clone and linked worktrees.",
        flush=True,
    )


def pushed_head(root: Path, updates: str) -> str | None:
    """Reject pushes that would validate a different tree from the outgoing tip."""
    commits = set()
    for line in updates.splitlines():
        fields = line.split()
        if len(fields) != 4:
            raise RuntimeError("Malformed Git pre-push update record.")
        local_ref, local_sha, _remote_ref, remote_sha = fields
        if not all(
            re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", sha)
            for sha in (local_sha, remote_sha)
        ):
            raise RuntimeError("Malformed Git pre-push object ID.")
        if set(local_sha) == {"0"}:
            continue
        try:
            commits.add(git(root, "rev-parse", "--verify", local_sha + "^{commit}"))
        except subprocess.CalledProcessError as error:
            raise RuntimeError(
                f"Cannot validate non-commit ref {local_ref}."
            ) from error
    if not commits:
        return None
    head = git(root, "rev-parse", "HEAD")
    if commits != {head}:
        raise RuntimeError(
            "Push tips must resolve to the checked-out HEAD. Check out and "
            "validate each branch separately; do not bypass the hook.",
        )
    require_clean(root, head)
    return head


def require_clean(root: Path, head: str) -> None:
    if git(root, "rev-parse", "HEAD") != head:
        raise RuntimeError("HEAD changed during validation; push aborted.")
    if git(root, "status", "--porcelain", "--untracked-files=all"):
        raise RuntimeError(
            "Pre-push requires a clean worktree and index, including untracked "
            "files, so tests validate the outgoing commit. Commit or stash first.",
        )


def stop_process(process: subprocess.Popen | None, job=None) -> None:
    try:
        if job is not None:
            job.terminate()
        elif process is not None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    finally:
        try:
            if process is not None:
                # An assignment failure leaves the Windows wrapper outside its
                # job, still waiting for permission to start the test command.
                if job is not None and process.poll() is None:
                    process.kill()
                process.wait(timeout=CLEANUP_TIMEOUT)
        finally:
            try:
                if process is not None and process.stdin is not None:
                    process.stdin.close()
            finally:
                if job is not None:
                    job.close()


def run_step(root: Path, step: dict, output: Path) -> None:
    timeout = step.get("timeout_seconds", 1200)
    if (
        isinstance(timeout, bool)
        or not isinstance(timeout, (int, float))
        or not math.isfinite(timeout)
        or timeout <= 0
    ):
        raise RuntimeError(
            f"{step['name']} requires a positive finite timeout_seconds."
        )
    replacements = {
        "{python}": sys.executable,
        "{output}": str(output),
        "{gate}": str(GATE),
        "{root}": str(root),
        "{codex}": str(
            root
            / "codex-rs/target/debug"
            / ("codex.exe" if os.name == "nt" else "codex")
        ),
    }
    args = [replacements.get(arg, arg) for arg in step["argv"]]
    executable = shutil.which(args[0])
    if executable is None:
        raise RuntimeError(
            f"Missing executable {args[0]}; see the Local CI instructions in AGENTS.md."
        )
    args[0] = executable
    cwd = root / step.get("cwd", ".")
    env = dict(os.environ)
    # Git exports repository-local variables to hooks; they must not redirect
    # Git commands inside test fixtures or dependency modules.
    for key in (
        "GOOS",
        "GOARCH",
        "CGO_ENABLED",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_PREFIX",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_NAMESPACE",
        "ZAI_AGENT_STATUS_ADDR",
        "ZAI_AGENT_STATUS_TOKEN",
    ):
        env.pop(key, None)
    env.update(
        {
            key: replacements.get(value, value)
            for key, value in step.get("env", {}).items()
        }
    )
    env["CODEX_REPO_ROOT"] = str(root)
    env["PYTHONUNBUFFERED"] = "1"
    print(f"\n== {step['name']} ==\n{' '.join(args)}", flush=True)
    options = {"stdin": subprocess.DEVNULL}
    job = None
    if os.name == "nt":
        from local_ci_windows import WindowsJob

        # A venv's redirector spawns the interpreter before we can assign it.
        # Start the gated wrapper with CPython's base executable instead.
        wrapper_python = getattr(sys, "_base_executable", None)
        if not wrapper_python:
            raise RuntimeError(
                "Cannot locate the base Python interpreter for process ownership."
            )
        job = WindowsJob()
        args = [
            wrapper_python,
            "-B",
            str(Path(__file__).with_name("local_ci_windows.py")),
            *args,
        ]
        options["stdin"] = subprocess.PIPE
        options["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
    else:
        options["start_new_session"] = True
    process = None
    failure = None
    try:
        process = subprocess.Popen(args, cwd=cwd, env=env, **options)
        if job is not None:
            job.assign(process.pid)
            process.stdin.write(b"go\n")
            process.stdin.close()
        code = process.wait(timeout=timeout)
        if code:
            failure = f"{step['name']} failed with exit code {code}; push blocked."
    except subprocess.TimeoutExpired:
        failure = f"{step['name']} timed out after {timeout:g}s; push blocked."
    except KeyboardInterrupt:
        failure = f"{step['name']} interrupted; push blocked."
    except OSError as error:
        failure = f"{step['name']} could not run: {error}; push blocked."
    finally:
        try:
            if failure:
                try:
                    print(failure, file=sys.stderr, flush=True)
                except (OSError, ValueError) as error:
                    failure += f" Could not write diagnostic: {error}."
        finally:
            try:
                stop_process(process, job)
            except (OSError, subprocess.TimeoutExpired) as error:
                raise RuntimeError(
                    f"{failure or step['name']}: process cleanup failed: {error}; push blocked.",
                ) from error
    if failure:
        raise RuntimeError(failure)


def changed_paths(root: Path, updates: str) -> list[str]:
    paths = set()
    for line in updates.splitlines():
        _local_ref, local_sha, _remote_ref, remote_sha = line.split()
        if set(local_sha) == {"0"}:
            continue
        if set(remote_sha) == {"0"}:
            result = git(root, "ls-tree", "-r", "--name-only", "-z", local_sha)
        else:
            # Missing remote objects must block the push, not omit checks.
            result = git(
                root, "diff", "--name-only", "--no-renames", "-z", remote_sha, local_sha
            )
        paths.update(path for path in result.split("\0") if path)
    return sorted(paths)


def applies(step: dict, paths: list[str] | None) -> bool:
    patterns = step.get("paths")
    return (
        paths is None
        or patterns is None
        or any(
            fnmatch.fnmatchcase(path, pattern) for path in paths for pattern in patterns
        )
    )


def validate(root: Path, paths: list[str] | None = None) -> None:
    config_path = root / ".github/local-ci.json"
    if not config_path.exists():
        config_path = GATE.parent / "local-ci.json"
    config = json.loads(config_path.read_text(encoding="utf-8"))
    platform = {"win32": "windows", "darwin": "darwin", "linux": "linux"}.get(
        sys.platform
    )
    if platform is None:
        raise RuntimeError(f"Unsupported local validation platform: {sys.platform}")
    skipped = []
    with tempfile.TemporaryDirectory(prefix="codex-local-ci-") as directory:
        for step in config["steps"]:
            if platform not in step.get("platforms", ["windows", "darwin", "linux"]):
                skipped.append(step["name"] + " (other platform)")
                continue
            if not applies(step, paths):
                skipped.append(step["name"] + " (unchanged area)")
                continue
            run_step(root, step, Path(directory))
    print(f"\nLocal validation passed on {platform}.", flush=True)
    print(
        "This establishes native-host evidence only; hosted Actions are disabled.",
        flush=True,
    )
    if skipped:
        print("Not run: " + ", ".join(skipped), flush=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--install", action="store_true", help="install the pre-push hook"
    )
    mode.add_argument(
        "--pre-push", action="store_true", help="consume Git ref updates on stdin"
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=ROOT,
        help="checkout validated by an installed hook",
    )
    args = parser.parse_args(argv)
    root = args.root.resolve()
    try:
        if args.install:
            install(root)
            return 0
        head = None
        paths = None
        if args.pre_push:
            updates = sys.stdin.read()
            head = pushed_head(root, updates)
            if head is None:
                print("No outgoing commit tips to validate (deletion-only push).")
                return 0
            paths = changed_paths(root, updates)
        validate(root, paths)
        if head is not None:
            require_clean(root, head)
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Local validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
