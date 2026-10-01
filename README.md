# zai-codex

A custom build of [OpenAI Codex](https://github.com/openai/codex) with GitHub
Copilot subscription support. Telemetry and remote diagnostic uploads are
disabled; local diagnostic logs remain available.

Download and install the latest customized build (Python 3.10+ required):

Linux / macOS:
```shell
curl -fsSL https://raw.githubusercontent.com/phoenixzqy/zai-codex/zai-codex/scripts/install_zai_codex.py | python3 -
```

Windows (PowerShell):
```powershell
irm https://raw.githubusercontent.com/phoenixzqy/zai-codex/zai-codex/scripts/install_zai_codex.py | python -
```

Run `zai-codex` from `~/.local/bin` (add that directory to PATH if needed).
Sign in with your own GitHub account and an eligible Copilot subscription.
The installer checks SHA-256 and keeps the original `codex` command intact.
These commands download and execute this repository's installer; review it first.

The download becomes available when the first [custom release](https://github.com/phoenixzqy/zai-codex/releases)
is published. See [release instructions](RELEASING.md) or the
[build documentation](README.backup.md) to build from source.

Licensed under [Apache 2.0](LICENSE). This is an independent fork, not an official
OpenAI release.
