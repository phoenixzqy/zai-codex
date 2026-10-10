# Custom releases

Build from clean, freshly fetched **`origin/zai-codex`**, never `main` (the upstream mirror). Source PRs target `zai-codex` and carry `zai`. The manually triggered `.github/workflows/zai-codex-release.yml` publishes packages and `zai-codex-v<version>` tags in **`phoenixzqy/zai-codex`**, targeting the dispatch source commit. All upstream workflows remain disabled. Local pre-push validation remains required.

After the release tools merge, claim a source worktree and install the [native prerequisites](README.backup.md). Build the same commit/version on each advertised native host. Start with custom version `0.1.0`, independently of the upstream executable version. `codex-package.json.version` must match the executable; the distribution version lives in `zai-release.json.version` and the custom release tag. A mismatch prevents the normal background-server launch.

```shell
python3 -B .github/scripts/local_ci.py --install
python3 -B -m unittest discover -s scripts -p 'test_*zai_codex*.py'
python3 -B scripts/prepare_zai_codex_release.py --version 0.1.0 --third-party-notices /path/to/reviewed-notices --output /path/to/release-assets
```

Use `python` on Windows. Output is `zai-codex-<version>-<target>.zip` plus a bare SHA-256 sidecar. The native packager retains the canonical bundle, Apache LICENSE/NOTICE, modification attribution, reviewed third-party notices and `zai-release.json` provenance; it smoke-tests version and Copilot login help. It neither uploads nor certifies notice completeness.

Audit actual Rust/native dependencies and bundled resources, including Ratatui, WezTerm-derived code, V8, ripgrep, bubblewrap, patched zsh and OpenSSL when linked. Optional voice libraries and Microsoft DLLs have separate redistribution obligations; voice runtime is not bundled by default. Identify modifications when distributing source.

Test interactive Copilot login, launch and installation on every native target. GNU Linux needs compatible glibc, not Alpine/musl. Complete signing/notarization as applicable, report actual signing state, and advertise only verified packages. Cross-compilation does not establish native correctness.

Follow the website's [publishing contract](https://github.com/phoenixzqy/phoenixzqy.github.io/blob/main/releases/README.md) and [zai-codex instructions](https://github.com/phoenixzqy/phoenixzqy.github.io/blob/main/releases/zai-codex/README.md): sync only `scripts/install_zai_codex.py` into its template, regenerate site installers, trigger the source release workflow, verify unauthenticated downloads, and publish final sizes/hashes/signing states in its manifest. Never copy the source checkout/archive into the site. The site links source-repo ZIPs directly; old website releases remain immutable.

Python 3.10+ installers consume the site's `releases/zai-codex/latest/manifest.json`, retain old complete bundles, and install the `codex` command (`codex.cmd` on Windows). SHA-256 verifies bytes, not publisher signing. Test upgrades with isolated `ZAI_INSTALL_DIR` (launcher directory, bundles under `releases/zai-codex`), or `ZAI_CODEX_INSTALL_ROOT` and `ZAI_CODEX_BIN_LINK`. Remove task-owned staging/build artifacts after verification, preserve shared caches, and release worktrees. Future releases repeat this workflow.

Release-only packaging does not require rebuilding or testing unrelated workspace components. Reuse a clean build of the same Rust sources when only release scripts or documentation change; verify the package version, Copilot login help, default launch, and isolated installation. Rust source changes still require the applicable native gate. Never bypass a pre-push hook.

## Manual GitHub release workflow

Enable only `zai-codex-release.yml`. Dispatch it on `zai-codex` with a fresh custom `version`; `source_commit` optionally checks the dispatch SHA and `automation_request_id` identifies website automation attempts. Native hosted runners build Linux GNU, macOS, and Windows MSVC packages for both x64 and ARM64. Publication requires all six jobs, checksum/provenance/archive checks, and native isolated installation smoke tests. Builds are unsigned and macOS packages are not notarized. Interactive Copilot login and the full source suite are not part of this release workflow.

`scripts/zai-codex-notices.json` pins previously published reviewed license material and the Cargo/ripgrep/zsh dependency inputs it covers. Dependency changes stop packaging until that review and pin are refreshed. This pin verifies the reviewed input, not legal completeness for newly introduced dependencies. The workflow publishes a `manifest.json` alongside ZIPs and SHA-256 sidecars; website automation verifies all six public packages and syncs the reviewed installer before updating its catalog. Failed publication may leave a draft; inspect and reconcile it without overwriting versioned bytes before another attempt.
