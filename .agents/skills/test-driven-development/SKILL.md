---
name: test-driven-development
description: Implement Rolvya behavior with test-first red/green/refactor cycles and explicit invariant checks. Use for Rust domain logic, SQLite boundaries, backend APIs and frontend behavior; not to introduce unapproved architecture.
---

# Test-driven development for Rolvya

Reviewed Rolvya adaptation of [Superpowers test-driven-development](https://github.com/obra/superpowers/blob/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/test-driven-development/SKILL.md) (MIT; pinned revision `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`). Self-contained instructions; no third-party executables.

- Confirm the acceptance contract and relevant ADRs. Query Graphify to find existing tests and callers; inspect implementation, not just the generated graph.
- Begin with one observable behavioral requirement, including a failure/denial case as appropriate. Write a deterministic test that fails **for the intended missing behavior**, not due to a broken test environment.
- Run the test and record that RED observation. Introduce the minimum implementation, then run the same test to demonstrate GREEN.
- Refactor only after green. Keep public semantics, typed IDs and transaction contracts stable, and rerun the focused test.
- Add boundary cases: malformed identifiers, cycles, duplicate task dependencies, immutable activated plans, integer overflow, stale worker lease/fencing, idempotent completion, denied spending and SQLite rollback as applicable.
- Use disposable databases and fake providers; don't create paid AWS jobs, call provider APIs, or use real credentials.
- Run full applicable language validation; Rust requires fmt, clippy and locked tests when scaffolded. Frontend requires lint, typecheck, test, build and exact artifact inspection when scaffolded.
- If build scaffolding cannot reasonably begin with a failing runtime test, first add CI/manifest checks, explain the exception and verify the actual artifact.
- Update documentation/status with implemented facts. Refresh the graph after editing source and hand off observed test output, not predictions.

Never rewrite existing implementation solely to comply with a generic TDD ritual; retain the smallest reliable evidence path for the current task.
