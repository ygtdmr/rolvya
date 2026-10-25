---
name: systematic-debugging
description: Diagnose Rolvya bugs, failing CI, anomalous worker states and integration errors using reproducible evidence and root-cause isolation before applying minimal fixes.
---

# Systematic debugging for Rolvya

Reviewed Rolvya adaptation of [Superpowers systematic-debugging](https://github.com/obra/superpowers/blob/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/systematic-debugging/SKILL.md) (MIT, source revision `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`). No dependency or executable script is installed.

1. Read `AGENTS.md`, relevant closed decisions, issue acceptance criteria and exact failing CI/test logs.
2. Query Graphify for the affected call chain using `graphify query`, then inspect actual source, data flow and related commits. Use `graphify path` or `graphify explain` if relationships are unclear.
3. Reproduce in the smallest controlled local/CI fixture. Capture exact inputs, outputs and error, then identify the first failing component boundary. Never print secrets or access live paid workers to diagnose by default.
4. Compare a working path with the failed path. State a falsifiable root-cause hypothesis and test it by changing one factor. Do not stack random fixes or remove safeguards just to clear CI.
5. Add a regression test demonstrating failure **before** implementing the minimal fix; preserve approved Rust-only backend, SQLite authority, immutable DAG, hard budget gate and fencing semantics.
6. Run the focused test, then the relevant full CI contracts. For source changes refresh Graphify using `graphify update .`.
7. If the hypothesis fails, return to evidence. Stop after the approved maximum four implementation attempts; escalate with the concrete blocker rather than improvise architecture changes.
8. Report root cause, reproduction, changed paths, tests passed/failed/skipped and residual risk. Never claim a fix based only on a green unrelated check.

Precedence: Rolvya's approved ADRs, repo CI, fixed upfront task DAG and Owner-only authority override any generic upstream development guidance.
