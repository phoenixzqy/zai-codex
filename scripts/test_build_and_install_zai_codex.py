#!/usr/bin/env python3

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("build_and_install_zai_codex.py")
SPEC = importlib.util.spec_from_file_location("build_and_install_zai_codex", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


class InstallDestinationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary_directory.cleanup)
        self.root = Path(self.temporary_directory.name)
        self.install_root = self.root / "packages"
        self.bin_link = self.root / "bin" / "zai-codex"

    def test_new_destination_preserves_upstream_codex(self) -> None:
        upstream_binary = self.bin_link.with_name("codex")
        upstream_binary.parent.mkdir(parents=True)
        upstream_binary.write_text("upstream")

        self.assertEqual(
            installer.install_root_and_link(self.install_root, self.bin_link),
            (self.install_root, self.bin_link, None),
        )
        self.assertEqual(upstream_binary.read_text(), "upstream")

    def test_reuses_only_own_previous_install(self) -> None:
        previous_package = self.install_root / "previous"
        executable = previous_package / "bin" / "codex"
        executable.parent.mkdir(parents=True)
        executable.write_text("codex")
        self.bin_link.parent.mkdir(parents=True)
        self.bin_link.symlink_to(executable)

        self.assertEqual(
            installer.install_root_and_link(self.install_root, self.bin_link),
            (self.install_root, self.bin_link, previous_package),
        )

    def test_refuses_foreign_symlink(self) -> None:
        self.bin_link.parent.mkdir(parents=True)
        self.bin_link.symlink_to(self.root / "foreign-codex")

        with self.assertRaisesRegex(ValueError, "symlink outside"):
            installer.install_root_and_link(self.install_root, self.bin_link)

    def test_refuses_regular_executable(self) -> None:
        self.bin_link.parent.mkdir(parents=True)
        self.bin_link.write_text("do not replace")

        with self.assertRaisesRegex(ValueError, "existing executable"):
            installer.install_root_and_link(self.install_root, self.bin_link)

    def test_activation_refuses_destination_created_during_build(self) -> None:
        installer.install_root_and_link(self.install_root, self.bin_link)
        self.bin_link.write_text("created during build")

        with self.assertRaisesRegex(ValueError, "existing executable"):
            installer.activate_install(
                self.install_root, self.bin_link, self.install_root / "new", None
            )
        self.assertEqual(self.bin_link.read_text(), "created during build")

    def test_activation_refuses_concurrent_package_switch(self) -> None:
        installer.install_root_and_link(self.install_root, self.bin_link)
        self.bin_link.symlink_to(self.install_root / "concurrent/bin/codex")

        with self.assertRaisesRegex(ValueError, "changed during the build"):
            installer.activate_install(
                self.install_root, self.bin_link, self.install_root / "new", None
            )
        self.assertEqual(
            self.bin_link.readlink(), self.install_root / "concurrent/bin/codex"
        )

    def test_failed_activation_preserves_previous_install(self) -> None:
        previous_package = self.install_root / "previous"
        installer.install_root_and_link(self.install_root, self.bin_link)
        self.bin_link.symlink_to(previous_package / "bin/codex")

        with patch.object(Path, "replace", side_effect=OSError("activation failed")):
            with self.assertRaisesRegex(OSError, "activation failed"):
                installer.activate_install(
                    self.install_root,
                    self.bin_link,
                    self.install_root / "new",
                    previous_package,
                )
        self.assertEqual(self.bin_link.readlink(), previous_package / "bin/codex")
        self.assertEqual(list(self.bin_link.parent.glob("*.tmp.*")), [])

    def test_activation_uses_target_executable_name(self) -> None:
        installer.install_root_and_link(self.install_root, self.bin_link)
        package = self.install_root / "windows-package"
        executable = package / "bin/codex.exe"
        executable.parent.mkdir(parents=True)
        executable.write_text("codex")
        installer.activate_install(
            self.install_root, self.bin_link, package, None, "codex.exe"
        )
        self.assertEqual(self.bin_link.resolve(), executable)


if __name__ == "__main__":
    unittest.main()
