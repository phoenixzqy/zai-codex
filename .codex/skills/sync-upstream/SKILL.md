---
name: sync-upstream
description: Sync openai/codex main into this fork's upstream mirror and then its default zai-codex branch, preserving fork customizations when conflicts arise.
---

# Sync upstream into zai-codex

`upstream` is `https://github.com/openai/codex.git`; `origin` is
`https://github.com/phoenixzqy/zai-codex.git`. `main` is an upstream-only mirror.
The default development branch, `zai-codex`, owns the fork's customizations and
is the source of truth for their intent during conflict resolution.

Use this skill when asked to sync or update this fork from upstream. Syncing is
not permission to force-push, discard work, enable Actions, or publish a release.
Follow the root AGENTS.md and claim an isolated worktree with the git-worktree
skill before any mutation. Leave other sessions' branches and checkouts alone.

1. Inspect remote URLs, worktree ownership, cleanliness, and unpushed commits.
   Fetch `upstream main` and `origin main zai-codex` inside the claimed worktree.
   Record the fetched commit IDs so validation and pushes use those exact tips.
2. Update the fork's `main` mirror from upstream with a fast-forward only.
   Start a task-local mirror branch at the fetched `origin/main` and merge the
   fetched `upstream/main` using `--ff-only`. Stop if it has fork-only commits or
   has diverged; preserve history and report what prevented fast-forwarding.
   If local `main` is free, fast-forward it to this mirror tip. If another
   worktree holds `main`, use the task-local mirror instead; do not move that
   checkout or its branch ref behind its back. Report any deferred local update.
3. Start a task-local integration branch at the fetched `origin/zai-codex`.
   Merge the updated local mirror into it. This is the second stage: source
   upstream -> local main mirror -> default fork branch. Do not merge upstream
   directly in a way that leaves the mirror behind.
4. Resolve every conflict before continuing. Compare both versions and the
   merge base. Preserve the `zai-codex` behavior and fork-owned instructions,
   local hook/config, authentication/privacy changes, and skills when they
   conflict with upstream. Port those customizations onto compatible upstream
   APIs and keep non-conflicting upstream fixes. Do not blindly replace whole
   files with ours or use an ours merge strategy; that can drop upstream fixes
   outside the conflict. Review generated files and regenerate them with their
   repository tooling when required. Inspect `git diff --check` and verify there
   are no unresolved entries with `git ls-files -u`.
5. Complete the merge, install the pre-push hook, and validate the merged result
   using the local CI gate. Follow the repo's targeted test and formatting rules;
   obtain authorization before an otherwise-unrequested complete Rust suite.
   Keep GitHub Actions disabled in the fork's settings even if the merge adds
   upstream workflows. Native platform gaps must be reported, not called passes.
6. Push only when the sync request authorizes publishing. Check remote tips have
   not moved. With the corresponding validated tip checked out and a clean
   worktree, push the mirror as `HEAD:main`, then the integration as
   `HEAD:zai-codex`, without force. Each push must pass its hook; the installed
   fallback gate covers the mirror even though its upstream tree intentionally
   lacks fork-only hook files. Never add fork files to `main` to install a hook.
   If a tip moves or either push fails, refresh and re-integrate rather than
   overwrite it. Report partial publication if main succeeds but zai-codex fails.
7. Report source and resulting commit IDs, conflict decisions, validation,
   publication, and any deferred local-main update. Remove task-owned disposable
   artifacts and release the worktree through the git-worktree skill.
