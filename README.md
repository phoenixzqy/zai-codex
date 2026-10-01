# zai-codex

A custom [OpenAI Codex](https://github.com/openai/codex) build with GitHub Copilot subscription support. Telemetry and remote diagnostic uploads are disabled; local logs remain available.

Download and install (Python 3.10+ required):

```shell
curl -fsSL https://phoenixzqy.github.io/install/zai-codex.sh | sh
```

Windows (PowerShell):
```powershell
irm https://phoenixzqy.github.io/install/zai-codex.ps1 | iex
```

Add `~/.local/bin` to PATH if needed and run `zai-codex` (`zai-codex.cmd` on Windows). Sign in with your own eligible GitHub Copilot account. The installer checks SHA-256 and preserves `codex`; review the script before running it.

Downloads become available after the first [custom release](https://phoenixzqy.github.io/apps/releases/?id=zai-codex). See [release instructions](RELEASING.md) or [build documentation](README.backup.md).

Licensed under [Apache 2.0](LICENSE). An independent fork, not an official OpenAI release.
