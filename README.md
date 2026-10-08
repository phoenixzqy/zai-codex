# zai-codex

A custom [OpenAI Codex](https://github.com/openai/codex) build with GitHub Copilot subscription support. Telemetry and remote diagnostic uploads are disabled; local logs remain available.

## Fork features

- **GitHub Copilot subscriptions.** Sign in with your own GitHub account and an eligible Copilot subscription.
- **CLI image rendering.** Preview viewed and generated images directly in the terminal with Kitty graphics, Sixel, or iTerm2 image support. Unsupported terminals retain text output.
- **Remote telemetry and diagnostics disabled.** Telemetry, remote feedback and error-report uploads are disabled; local logs remain available.

## Install

Download and install (Python 3.10+ required):

```shell
curl -fsSL https://phoenixzqy.github.io/install/zai-codex.sh | sh
```

Windows (PowerShell):
```powershell
irm https://phoenixzqy.github.io/install/zai-codex.ps1 | iex
```

Add `~/.local/bin` to PATH if needed and run `codex` (`codex.cmd` on Windows). Sign in with your own eligible GitHub Copilot account. The installer checks SHA-256 and installs this fork as `codex` and retains previous fork bundles; review the script before running it.

Downloads become available after the first [custom release](https://phoenixzqy.github.io/apps/releases/?id=zai-codex). See [release instructions](RELEASING.md) or [build documentation](README.backup.md).

Licensed under [Apache 2.0](LICENSE). An independent fork, not an official OpenAI release.
