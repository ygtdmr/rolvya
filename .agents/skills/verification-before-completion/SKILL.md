---
name: verification-before-completion
description: Require fresh, exact verification evidence before a Codex agent reports a Rolvya task as finished, passes tests, opens a PR, or recommends merge.
---

# Verification before completion — Rolvya gate

Reviewed adaptation of [Superpowers verification-before-completion](https://github.com/obra/superpowers/blob/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/verification-before-completion/SKILL.md) (MIT; pinned revision `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`).

1. Identify exactly which requirement or claim is being verified, and which command or external GitHub result can substantiate it.
2. Run all applicable checks on the current commit, read exit codes and failures, and record the result. A previously passing test or a colleague's statement is not fresh evidence.
3. Validate real source and tests against Graphify-derived expectations; use Graphify to navigate, never to certify correctness. Update its graph after source changes.
4. Distinguish skipped language jobs, passing aggregate CI, locally passing tests, reviewed security checks, deployed services and verified acceptance; none implies the others.
5. For Rust, record fmt, clippy, test and lockfile evidence. For frontend, record test/build/browser and the one-JS-containing-CSS artifact check. For SQLite, record migration replay and failure-injection evidence when relevant.
6. For PRs, verify exact head commit, changed-file inventory, no sensitive path in the automatic merge allowlist, and current required statuses. Never grant yourself human approval or bypass protected `main`.
7. If a tool or live provider isn't accessible, state that verification is missing and do not manufacture success. Do not start billable external tests without explicit authorization.
8. Provide a concise factual handoff: what changed, commands and counts, failures and skips, unverified risks, link or commit and recommended next step.

A PR or a passing CI run is not proof that the entire Rolvya product feature is implemented.
