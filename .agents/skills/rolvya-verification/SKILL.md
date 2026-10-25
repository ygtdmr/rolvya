---
name: rolvya-verification
description: Write or evaluate Rolvya automated tests, deterministic fake-worker scenarios, failure injection, CI evidence, and feature acceptance reports. Use whenever new behavior needs proof or a PR claims readiness; not as a substitute for implementing the behavior.
---

# Rolvya verification

1. Identify the changed behavior and its approved requirement/ADR and acceptance cases. Check `docs/product/acceptance-criteria.md`, `docs/development/testing.md`, and the relevant domain test matrix. Separate planned validation from executed evidence.
2. Start with Graphify query/path/explain for the impacted call chain; inspect actual source and existing tests. Use generated graph output as navigation, not as proof.
3. Create the smallest deterministic test first, then cover negative and boundary behavior: malformed inputs, overflow, cross-project permissions, cycles, duplicate events, idempotency, stale lease/fencing, retries, crashes, budget exhaustion, late provider charges and missing secrets as applicable.
4. Prefer isolated fake clock, fake cloud provider, fake Git adapter and disposable SQLite. Do not provision paid AWS workers, call live model APIs, use real credentials, or invent production infrastructure for a test.
5. Verify critical invariants at the appropriate layer: pure Rust unit tests for domain checks; transactional SQLite tests for atomicity; fake-provider integration tests for orchestration; read-only browser tests for the UI. CI must fail if an applicable language test job was skipped.
6. Test not only intended success but also that prohibited actions are rejected and leave no unauthorized durable effects. For security-sensitive changes include an explicit bypass/permission and rollback assessment.
7. Run the actual available checks, including format/lint/typecheck/test/build/CI artifact checks when applicable. Do not describe a green aggregate as proof for a missing component or untested browser/cloud path.
8. Report a compact evidence table: requirement, test, command, pass/fail/skip, environment, and outstanding risk. Update project status only with verified facts and refresh Graphify after source changes.

Relevant contracts: `docs/development/remote-integration-test-matrix.md`, `docs/development/rust-sqlite-test-matrix.md`, and `docs/project/milestones.md`.
