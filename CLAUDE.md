# Claude Code instructions for Rolvya

[AGENTS.md](AGENTS.md) is the single authoritative agent protocol for this repository. It is imported in full below; this file only adapts Codex-oriented mechanics to Claude Code. If anything here appears to conflict with `AGENTS.md`, approved ADRs or `docs/project/status.md`, those win. Report the discrepancy instead of resolving it silently.

@AGENTS.md

## Claude Code adapter

### Shared skills (no duplicated content)
`.claude/skills/` holds **relative symlinks** into the canonical skill sources, so Codex and Claude Code load identical procedures:

| Claude skill | Canonical source |
| --- | --- |
| `rolvya-rust-foundation`, `rolvya-sqlite-integrity`, `rolvya-verification`, `rolvya-connected-ui`, `rolvya-pr-risk-review` | `.agents/skills/<name>/` |
| `systematic-debugging`, `test-driven-development`, `verification-before-completion`, `react-best-practices`, `web-design-guidelines`, `caveman` | `.agents/skills/<name>/` (reviewed external adaptations; see [skills README](.agents/skills/README.md)) |
| `graphify` | `.codex/skills/graphify/` |

- Edit skills only at their canonical source. Never replace a symlink with a copy.
- The safety and precedence rules in [.agents/skills/README.md](.agents/skills/README.md#safety-and-precedence) apply unchanged. Do not install or run third-party skill scripts, hooks, proxies or `npx` installers.
- Skill text that says "Codex agent" or "Codex Cloud" applies equally to a Claude Code session.

### Graphify under Claude Code
- `AGENTS.md` uses Codex syntax (`$graphify .`). In Claude Code, invoke the `graphify` skill (`/graphify .`) to build `graphify-out/` when `graphify-out/graph.json` is missing.
- The `graphify query|path|explain|update` CLI rules in `AGENTS.md` apply as written. If the `graphify` CLI is not on `PATH`, say so explicitly and fall back to direct source inspection. Never claim graph results you did not obtain.
- When you spawn exploring subagents, include the graph-first rule in their prompts, as `AGENTS.md` requires.
- `.codex/hooks.json` runs `graphify hook-check` before Bash; status.md documents it as an intentional no-op. No Claude Code hook equivalent is configured yet.

### Git, branches and PRs
- [`.github/integration-policy.json`](.github/integration-policy.json) pins the **`codex/` branch prefix** for the single-account model. Use `codex/<topic>` branches for Claude Code PRs as well. Do not invent a `claude/` prefix or edit the policy to accept one.
- Commit or push only when the user asks. Never self-approve, merge sensitive PRs, or bypass protection (see [PR integration](docs/development/pull-request-integration.md)).
- `CLAUDE.md` and `.claude/**` are dot or tooling paths outside the routine path allowlist, so changes to them need manual Owner review.

### Handoffs
End every substantive task with the handoff fields listed under "Task handoff requirements" in `AGENTS.md`: objective, files, decisions, exact tests run and not run, risks, and next steps.
