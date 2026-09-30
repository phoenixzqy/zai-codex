# zai-codex

This is a direct fork of `openai/codex`. `main` contains upstream code only;
`zai-codex` contains the customizations. The upstream README follows this section.

## Customizations

- GitHub Copilot device-code login, provider-scoped credentials, model discovery,
  reasoning capabilities, and terminal onboarding.
- Analytics collection and delivery disabled; OTLP log, trace, and metrics
  exporters disabled even when configured by the user.
- Remote feedback and error-report uploads disabled. Local diagnostic logs remain
  available; inference, authentication, MCP, and other requested network tools
  still work. This is not a network-isolation mode.
- Python release-package builder and atomic installer, separate from `codex`.
- Schema-aware `apply_patch` prompt guidance ported from codex-evo.

The auth and onboarding ports reference codex-evo commits `20b4f56b88b7`,
`f9ac942e2`, `4d3d1c2cd`, and `d0f4b0b02392`; patch guidance references
`67f5b0e79`. Upstream gateway, networking, and credential protections are retained.
Machine-specific configuration and unrelated fork defaults are not imported.

## Build and install

Install the native prerequisites described in the upstream build instructions,
including the pinned Rust toolchain, Python 3, OpenSSL development libraries, and
Linux libcap development libraries when building bubblewrap. Then run:

```shell
python3 scripts/build_and_install_zai_codex.py
~/.local/bin/zai-codex login github-copilot
```

On Linux the upstream package target defaults to musl. For a native GNU/Linux
toolchain, pass `--target x86_64-unknown-linux-gnu` (or
`--target aarch64-unknown-linux-gnu` on ARM64) instead.

The installer leaves `~/.local/bin/codex` untouched, validates the package before
activation, and keeps older packages in `~/.local/lib/zai-codex`. Override paths
with `ZAI_CODEX_INSTALL_ROOT` and `ZAI_CODEX_BIN_LINK`. Existing trusted resource
binaries can be supplied with `CODEX_BWRAP_BIN` and `CODEX_RG_BIN`; native OpenSSL
overrides use Cargo's standard `OPENSSL_DIR` and `OPENSSL_STATIC` variables.
To roll back, point `~/.local/bin/zai-codex` at an older package's `bin/codex`.
To use the custom build as `codex`, replace that command's symlink with the
installed `zai-codex` symlink; no rebuild is needed. For later installations,
set `ZAI_CODEX_BIN_LINK` to `~/.local/bin/codex` to keep updating that command.

## Branch maintenance

Keep `upstream` pointing to `https://github.com/openai/codex.git` and `origin`
pointing to your fork. Sync the fork's remote `main` without force-pushing:

```shell
gh repo sync phoenixzqy/zai-codex --source openai/codex --branch main
git fetch upstream main
git fetch origin main
```

In an isolated checkout of `main`, use `git merge --ff-only upstream/main`.
In an isolated checkout of `zai-codex`, merge `upstream/main`, resolve conflicts,
and revalidate the privacy/auth ports. Never put customization commits on `main`.
Default customization pull requests target `zai-codex`, not `main`.
Synchronization is explicit, not automatic: no custom scheduled workflow is
added to the pristine upstream branch.

---

<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src="https://github.com/openai/codex/blob/main/.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>

---

## Quickstart

### Installing and running Codex CLI

Run the following on Mac or Linux to install Codex CLI:

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

Run the following on Windows to install Codex CLI:

```shell
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
```

The standalone installers download from `https://releases.openai.com/codex` by default and fall back to GitHub Releases if a metadata or asset download is unavailable. To force GitHub Releases, set `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM` to `false` (`0` and `no` are also accepted):

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false sh
```

```powershell
$env:CODEX_INSTALLER_USE_RELEASES_OPENAI_COM='false'; irm https://chatgpt.com/codex/install.ps1 | iex
```

Codex CLI can also be installed via the following package managers:

```shell
# Install using npm
npm install -g @openai/codex
```

```shell
# Install using Homebrew
brew install --cask codex
```

Then simply run `codex` to get started.

<details>
<summary>You can also go to the <a href="https://github.com/openai/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (e.g., `codex-x86_64-unknown-linux-musl`), so you likely want to rename it to `codex` after extracting it.

</details>

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

### Using this fork with GitHub Copilot

Run `codex` and select **Sign in with GitHub Copilot** on the sign-in screen.
Open the displayed GitHub link, enter the one-time code, and authorize access.
Codex checks your Copilot access and available models, saves credentials using
the configured credential store, and shows a confirmation. Press Enter to
continue. Press Esc during sign-in to cancel; if authorization expires or fails,
select GitHub Copilot again to retry.

Existing users can run `codex logout` to return to the sign-in screen on the next
launch, or use `codex login github-copilot` directly. GitHub Copilot sign-in is
unavailable when workspace policy requires ChatGPT authentication.

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
