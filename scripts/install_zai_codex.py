#!/usr/bin/env python3
"""Download a verified custom release without replacing upstream Codex."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import urllib.error
import urllib.request
import zipfile


REPOSITORY = "phoenixzqy/zai-codex"
TAG_PATTERN = re.compile(r"zai-v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?")
WINDOWS_MARKER = "@rem zai-codex release launcher\n"


def host_target():
    system = platform.system()
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    suffix = {
        "Linux": "unknown-linux-gnu",
        "Darwin": "apple-darwin",
        "Windows": "pc-windows-msvc",
    }.get(system)
    if arch not in ("x86_64", "aarch64") or suffix is None:
        raise ValueError(f"Unsupported platform: {system} {machine}")
    return f"{arch}-{suffix}"


def download(url, destination):
    request = urllib.request.Request(url, headers={"User-Agent": "zai-codex-installer"})
    with urllib.request.urlopen(request, timeout=120) as response:
        with destination.open("wb") as output:
            shutil.copyfileobj(response, output)


def verified_extract(archive_path, checksum, destination):
    expected = checksum.strip()
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("Invalid release SHA-256")
    digest = hashlib.sha256()
    with archive_path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != expected:
        raise ValueError("Release SHA-256 mismatch")

    def output_path(name):
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\\" in name or ":" in name:
            raise ValueError(f"Unsafe archive path: {name}")
        result = destination.joinpath(*path.parts)
        if not result.resolve().is_relative_to(destination.resolve()):
            raise ValueError(f"Unsafe archive path: {name}")
        return result

    def copy_member(name, mode, source):
        path = output_path(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as output:
            shutil.copyfileobj(source, output)
        path.chmod(0o755 if mode & 0o111 else 0o644)

    if archive_path.suffix == ".zip":
        with zipfile.ZipFile(archive_path) as archive:
            for member in archive.infolist():
                mode = member.external_attr >> 16
                if stat.S_ISLNK(mode):
                    raise ValueError("Archive links are not allowed")
                if member.is_dir():
                    output_path(member.filename).mkdir(parents=True, exist_ok=True)
                else:
                    with archive.open(member) as source:
                        copy_member(member.filename, mode, source)
    else:
        with tarfile.open(archive_path, "r:gz") as archive:
            for member in archive:
                if member.isdir():
                    output_path(member.name).mkdir(parents=True, exist_ok=True)
                elif member.isfile():
                    with archive.extractfile(member) as source:
                        copy_member(member.name, member.mode, source)
                else:
                    raise ValueError("Archive links and special files are not allowed")


def validate_package(package, target, tag):
    metadata = json.loads((package / "codex-package.json").read_text())
    provenance = json.loads((package / "zai-release.json").read_text())
    windows = target.endswith("windows-msvc")
    suffix = ".exe" if windows else ""
    if any(
        metadata.get(key) != value
        for key, value in {
            "layoutVersion": 1,
            "target": target,
            "variant": "codex",
            "entrypoint": f"bin/codex{suffix}",
            "version": tag.removeprefix("zai-v"),
        }.items()
    ):
        raise ValueError("Release package metadata mismatch")
    if (
        provenance.get("repository") != REPOSITORY
        or provenance.get("branch") != "zai-codex"
        or provenance.get("tag") != tag
        or not re.fullmatch(r"[0-9a-f]{40}", provenance.get("commit", ""))
    ):
        raise ValueError("Custom release provenance mismatch")
    required = [
        f"bin/codex{suffix}",
        f"bin/codex-code-mode-host{suffix}",
        f"codex-path/rg{suffix}",
        "LICENSE",
        "NOTICE",
        "MODIFICATIONS.txt",
    ]
    if "linux" in target:
        required.append("codex-resources/bwrap")
    if windows:
        required.extend(
            [
                "codex-resources/codex-command-runner.exe",
                "codex-resources/codex-windows-sandbox-setup.exe",
            ]
        )
    if not all((package / name).is_file() for name in required):
        raise ValueError("Incomplete custom release package")
    if not (package / "third-party-notices").is_dir():
        raise ValueError("Missing third-party notices")
    binary = package / f"bin/codex{suffix}"
    subprocess.run([str(binary), "--version"], check=True)
    subprocess.run(
        [str(binary), "login", "github-copilot", "--help"],
        check=True,
        stdout=subprocess.DEVNULL,
    )


def launcher_state(launcher, root, windows):
    if launcher.is_symlink():
        if windows or not launcher.resolve().is_relative_to(root):
            raise ValueError(f"Refusing to replace a foreign launcher: {launcher}")
        return str(launcher.readlink())
    if launcher.exists():
        if windows and launcher.is_file():
            text = launcher.read_text()
            if text.startswith(WINDOWS_MARKER):
                return text
        raise ValueError(f"Refusing to replace an existing executable: {launcher}")
    return None


def install(archive, checksum, tag, target, root, launcher):
    root = root.expanduser().resolve()
    launcher = launcher.expanduser()
    launcher.parent.mkdir(parents=True, exist_ok=True)
    launcher = launcher.parent.resolve() / launcher.name
    windows = target.endswith("windows-msvc")
    previous = launcher_state(launcher, root, windows)
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".zai-stage-", dir=root) as temporary:
        package = Path(temporary) / "package"
        package.mkdir()
        verified_extract(archive, checksum, package)
        validate_package(package, target, tag)
        if launcher_state(launcher, root, windows) != previous:
            raise ValueError("Install destination changed during download")
        destination = root / f"{tag}-{target}-{Path(temporary).name}"
        package.rename(destination)
        try:
            with tempfile.TemporaryDirectory(
                prefix=".zai-launcher-", dir=launcher.parent
            ) as staging:
                staged = Path(staging) / launcher.name
                if windows:
                    binary = str(destination / "bin/codex.exe")
                    if any(char in binary for char in '%"\r\n'):
                        raise ValueError("Unsupported Windows install path")
                    staged.write_text(
                        WINDOWS_MARKER
                        + "@setlocal DisableDelayedExpansion\n"
                        + f'@"{binary}" %*\n'
                    )
                else:
                    staged.symlink_to(destination / "bin/codex")
                staged.replace(launcher)
        except BaseException:
            shutil.rmtree(destination)
            raise
    print(f"Installed {tag}: {launcher}")
    print(f"Add {launcher.parent} to PATH if needed, then run zai-codex.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", default="latest", help="latest or zai-vX.Y.Z")
    args = parser.parse_args()
    tag = args.release
    with tempfile.TemporaryDirectory(prefix="zai-codex-download-") as temporary:
        temporary = Path(temporary)
        if tag == "latest":
            metadata = temporary / "release.json"
            download(
                f"https://api.github.com/repos/{REPOSITORY}/releases/latest", metadata
            )
            tag = json.loads(metadata.read_text())["tag_name"]
        if not TAG_PATTERN.fullmatch(tag):
            raise ValueError(f"Not a custom zai-codex release: {tag}")
        target = host_target()
        extension = "zip" if target.endswith("windows-msvc") else "tar.gz"
        name = f"zai-codex-{target}.{extension}"
        base = f"https://github.com/{REPOSITORY}/releases/download/{tag}"
        archive = temporary / name
        checksum = temporary / f"{name}.sha256"
        download(f"{base}/{name}.sha256", checksum)
        download(f"{base}/{name}", archive)
        root = Path(os.environ.get("ZAI_CODEX_INSTALL_ROOT", "~/.local/lib/zai-codex"))
        launcher = Path(
            os.environ.get(
                "ZAI_CODEX_BIN_LINK",
                "~/.local/bin/zai-codex.cmd"
                if extension == "zip"
                else "~/.local/bin/zai-codex",
            )
        )
        install(archive, checksum.read_text(), tag, target, root, launcher)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except urllib.error.HTTPError as error:
        raise SystemExit(
            f"Custom release download failed ({error.code}); no upstream fallback."
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error))
