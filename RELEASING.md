# Custom releases

The default and release branch is **`zai-codex`**, not `main`. `main` mirrors
`openai/codex` and must never supply a customized release. Pull requests target
`zai-codex` and carry the `zai` label. Hosted Actions are disabled; releases are
built and validated locally, then uploaded with `gh`.

## Prepare native assets

After the release changes merge, claim an isolated worktree based on the freshly
fetched `origin/zai-codex`. Build the same clean commit and release version on
each advertised platform. Start with `0.1.0` for the first custom release;
increment the custom version for subsequent releases independently of the
upstream version embedded in `codex --version`.

Use the native prerequisites in [the build documentation](README.backup.md),
install the local hook, and run the complete local gate before publishing:

```shell
python3 -B .github/scripts/local_ci.py --install
python3 -B .github/scripts/local_ci.py
```

Prepare a reviewed directory containing applicable third-party license texts
and copyright notices. Audit the actual Rust/native dependency graph, including
Ratatui, WezTerm-derived code, V8, OpenSSL where linked, and the bundled ripgrep,
bubblewrap, and patched zsh resources where present. Preserve Apache LICENSE and
NOTICE attribution; prominently identify changed files when distributing source.
Do not assume the root Apache license covers every bundled component. The
packager requires this notice directory but cannot certify its completeness.
If adding native voice libraries or Microsoft runtime DLLs, fulfill their
separate LGPL/source/relinking or Microsoft redistribution requirements first.
The package builder does not add the optional voice runtime by default.

Run on each native host (replace `python3` with `python` on Windows):

```shell
python3 -B scripts/prepare_zai_codex_release.py --version 0.1.0 --third-party-notices /path/to/reviewed-notices --output /path/to/release-assets
```

The host determines the asset target: Linux x86_64/ARM64 GNU, macOS Intel/Apple
Silicon, or Windows x64/ARM64 MSVC. Advertise only targets actually built and
tested. Linux users need a glibc distribution compatible with the build host;
these GNU packages do not support Alpine/musl. Each asset includes the full
package layout, licenses, modification attribution, source commit, and SHA-256
sidecar. The command smoke-tests `--version` and Copilot login availability.
Perform an interactive Copilot login and launch on each native target too.
Configure any required Windows signing or macOS signing/notarization before
public distribution; do not claim unsigned packages are signed.

Keep assets from all hosts in one task-owned staging directory. Verify their
`zai-release.json` files all name the same commit and tag. Test installation and
upgrade using isolated `ZAI_CODEX_INSTALL_ROOT` and `ZAI_CODEX_BIN_LINK` paths;
ensure the original `codex` is preserved. The Unix launcher is a symlink;
Windows uses `zai-codex.cmd`, so developer-mode symlink permissions are not needed.

## Publish and update

Create and push an annotated `zai-v0.1.0` tag at that validated customization
commit from its claimed worktree. Do not bypass the pre-push gate.

```shell
git tag -a zai-v0.1.0 -m "zai-codex 0.1.0"
git push origin zai-v0.1.0
gh release create zai-v0.1.0 --repo phoenixzqy/zai-codex --verify-tag --draft --title "zai-codex 0.1.0" --notes-file /path/to/release-notes.md /path/to/release-assets/zai-codex-*.tar.gz /path/to/release-assets/zai-codex-*.zip /path/to/release-assets/zai-codex-*.sha256
```

Pass only verified asset paths; adjust the list to the targets being published.
Release notes should identify the source commit, supported hosts and runtime
requirements, native checks, signing status, and disabled remote telemetry / diagnostics.
Inspect the draft and its assets, then publish:

```shell
gh release edit zai-v0.1.0 --repo phoenixzqy/zai-codex --draft=false --latest
```

Test both README install commands against the published assets. They fetch the
latest custom tag once, verify its archive checksum before extraction/execution,
and never substitute `openai/codex` binaries. Rerunning installs an update;
previous packages remain available for rollback. For a specific release,
download the installer and run `python3 install_zai_codex.py --release zai-v0.1.0`.
The SHA-256 sidecar detects corruption; it is not independent publisher signing.

Remove task-owned staging/build/test directories after validation and upload;
retain installed packages and shared compiler caches. Release the claimed
worktree when done. Future releases follow this same branch and asset contract.
