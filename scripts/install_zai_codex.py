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
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import zipfile


REPOSITORY = "phoenixzqy/zai-codex"
SITE = "https://phoenixzqy.github.io"
TAG_PATTERN = re.compile(r"zai-codex-v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?")
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
    def check_url(value):
        parsed = urllib.parse.urlsplit(value)
        if parsed.scheme == "https" and not parsed.username and not parsed.password:
            return
        if (
            os.environ.get("ZAI_RELEASE_MANIFEST_URL")
            and parsed.scheme == "http"
            and parsed.hostname == "127.0.0.1"
        ):
            return
        raise ValueError("Downloads require HTTPS")

    class RedirectHandler(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, request, response, code, message, headers, newurl):
            check_url(newurl)
            if urllib.parse.urlsplit(newurl).scheme != "https":
                raise ValueError("HTTPS redirects must stay on HTTPS")
            if urllib.parse.urlsplit(request.full_url).scheme != "https":
                raise ValueError("Local test downloads must not redirect")
            return super().redirect_request(
                request, response, code, message, headers, newurl
            )

    check_url(url)
    request = urllib.request.Request(url, headers={"User-Agent": "zai-codex-installer"})
    with urllib.request.build_opener(RedirectHandler()).open(
        request, timeout=120
    ) as response:
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

    with zipfile.ZipFile(archive_path) as archive:
        for member in archive.infolist():
            mode = member.external_attr >> 16
            if stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR):
                raise ValueError("Archive links and special files are not allowed")
            if member.is_dir():
                output_path(member.filename).mkdir(parents=True, exist_ok=True)
            else:
                with archive.open(member) as source:
                    copy_member(member.filename, mode, source)


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
            "version": tag.removeprefix("zai-codex-v"),
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
    parser.parse_args()
    if sys.version_info < (3, 10):
        raise ValueError("Python 3.10+ is required")
    manifest_url = os.environ.get(
        "ZAI_RELEASE_MANIFEST_URL", f"{SITE}/releases/zai-codex/latest/manifest.json"
    )
    with tempfile.TemporaryDirectory(prefix="zai-codex-download-") as temporary:
        temporary = Path(temporary)
        metadata = temporary / "manifest.json"
        download(manifest_url, metadata)
        manifest = json.loads(metadata.read_text())
        if manifest.get("schemaVersion") != 1 or manifest.get("appId") != "zai-codex":
            raise ValueError("Custom release manifest mismatch")
        release = manifest["release"]
        if release is None:
            raise ValueError("No public release of zai-codex has been published yet")
        tag = f"zai-codex-v{release['version']}"
        if not TAG_PATTERN.fullmatch(tag):
            raise ValueError(f"Not a custom zai-codex release: {tag}")
        target = host_target()
        platform_id = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}[
            platform.system()
        ]
        architecture = "arm64" if target.startswith("aarch64") else "x64"
        matches = [
            asset
            for asset in release["assets"]
            if asset.get("platform") == platform_id
            and asset.get("architecture") == architecture
        ]
        if len(matches) != 1:
            raise ValueError(
                f"No unique published package for {platform_id}/{architecture}"
            )
        asset = matches[0]
        name = f"zai-codex-{release['version']}-{target}.zip"
        if asset["file"] != name:
            raise ValueError("Custom package filename mismatch")
        url = asset.get("url", urllib.parse.urljoin(manifest_url, name))
        allowed = f"https://github.com/phoenixzqy/phoenixzqy.github.io/releases/download/{tag}/{name}"
        local = urllib.parse.urljoin(manifest_url, name)
        if url != allowed and url != local:
            raise ValueError(
                "Package URL must use the website release or manifest folder"
            )
        archive = temporary / name
        download(url, archive)
        if (
            type(asset["bytes"]) is not int
            or asset["bytes"] <= 0
            or archive.stat().st_size != asset["bytes"]
        ):
            raise ValueError("Release byte count mismatch")
        install_dir = os.environ.get("ZAI_INSTALL_DIR")
        root = Path(
            os.environ.get(
                "ZAI_CODEX_INSTALL_ROOT",
                str(Path(install_dir) / "releases/zai-codex")
                if install_dir
                else "~/.local/lib/zai-codex",
            )
        )
        launcher = Path(
            os.environ.get(
                "ZAI_CODEX_BIN_LINK",
                str(
                    Path(install_dir or "~/.local/bin")
                    / (
                        "zai-codex.cmd"
                        if target.endswith("windows-msvc")
                        else "zai-codex"
                    )
                ),
            )
        )
        install(archive, asset["sha256"], tag, target, root, launcher)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except urllib.error.HTTPError as error:
        raise SystemExit(
            f"Custom release download failed ({error.code}); no upstream fallback."
        )
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        AttributeError,
        zipfile.BadZipFile,
        subprocess.SubprocessError,
    ) as error:
        raise SystemExit(str(error))
