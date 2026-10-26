# Rust core foundation

This is the implemented R01/R02 library boundary, not a running orchestrator or a production release. It follows [the task packages](initial-implementation-task-packages.md), [task model](../architecture/task-model.md), [group policy](../architecture/parent-task-aggregation.md) and [approved decisions](../decisions/README.md).

## Workspace and use

- `rolvya-domain`: opaque project/plan/task/attempt identifiers, nonzero plan versions, exact money and forecasts, flat static plans and separate mutable attempt records.
- `rolvya-plan-validator`: pure `validate(plan, policy, limits)`, structured diagnostics, deterministic explicit-dependency topological order, canonical bytes and SHA-256 fingerprints.

The validator borrows the draft immutably through `ValidatedPlan`; callers cannot edit that draft while its validated borrow exists. This is a Rust library guarantee, **not durable plan activation**. A topological order includes group nodes and is **not a dispatch queue**. Only executable leaves may eventually be dispatched by the scheduler.

The caller must explicitly implement `AssignmentPolicy` from trusted capability/entitlement evidence. No permissive production policy or implicit CLI-to-API fallback is supplied. Test fixtures allow synthetic assignments only. Structural validation does not prove provider permission, available budget, valid credentials or authorized activation.

IDs use 1–64 lowercase ASCII letters/digits/underscore/hyphen, beginning with a letter or digit; values are never trimmed or case-folded. Plan versions are positive `u64` values. Identifier and money validation errors do not echo rejected values; future transport adapters must redact parser diagnostics before exposing them. Reference scope contains project, plan **and version**. Unknown plan fields, unknown effort/modes and undeclared write-scope fields reject during decoding.

Money uses nonnegative `i64` minor units, a distinct signed correction type, exact currency identity and checked arithmetic. Three-uppercase-letter currency syntax is checked locally, without claiming ISO registry membership or performing conversion. All four categories remain separate. Scenario totals must satisfy optimistic ≤ expected ≤ pessimistic; per-category tradeoffs are allowed. Forecasts and signed corrections convey no spending authority or ledger provenance.

## Validation semantics and limits

Validation rejects empty/unsupported plans, duplicate IDs and declarations, cross-scope/dangling references, parent/self/dependency cycles, executable parents, empty/executable groups and all unsupported optional-child semantics. All nodes require bounded text, context SHA-256 and explicit acceptance criteria (including group aggregation acceptance in v1). Executables additionally require resources, four-category costs and an allowed planned assignment. Explicit artifact expectations must refer to direct predecessors.

Hierarchy remains separate from dependencies. Required-child completion constraints are analyzed internally to catch group-completion deadlocks (for example, a descendant waiting on its own ancestor group). Those constraints never become implicit edges in emitted dependency order or canonical content. Diagnostics identify actual cycle members, rather than every blocked downstream node.

Remote writes use a canonical lowercase `owner/repository`, `refs/heads/...` branch and relative literal paths. Shared targets with overlapping path prefixes need dependency/completion sequencing; otherwise validation rejects. Distinct branches or nonoverlapping literal paths provide declared isolation. These are planned scope declarations: no Git request, protected-ref permission or symlink-resolved filesystem isolation is implemented here. A future trusted publisher must enforce actual repository/ref/path authorization.

`Limits::default()` bounds each validation to 10,000 tasks, 100,000 relationship/collection entries, 32 MiB of user text and 1,000,000 conflict-analysis work units. These configurable validation budgets prevent unbounded index/graph/conflict work; they are not product nesting limits. Deep traversals are iterative. Callers must separately limit transport decoding before constructing the typed plan. Limit exhaustion fails closed.

Graph construction and iterative cycle analysis use O(V+E) space; ordered indexes/topological tie-breaking add logarithmic work. Conflict analysis uses an explicit work budget for path comparisons and reachability. No global quadratic reachability matrix is retained. No claim is made about a production controller's 2–4 GiB deployment target.

## Canonical format v1

The SHA-256 input is UTF-8 JSON containing `format: "rolvya-plan/v1"` and the complete validated static `PlanContent`. Struct field order and Serde representations are part of this versioned format. Tasks sort by ID; dependencies, scope, exclusions, acceptance criteria, policy refs, remote paths and artifact expectations sort deterministically. These arrays are semantic sets in v1. Text within each value is preserved byte-for-byte; no Unicode or whitespace normalization occurs. No hash-map iteration order is used.

All static identity, hierarchy, explicit prerequisites, instructions, context/artifacts, assignments, resources, write scopes and forecasts are included. Runtime attempts/status, model fallback, lease epoch, logs and actual charges are separate types and cannot enter the encoding. Invalid plans do not produce `ValidatedPlan` or a fingerprint. Future encoding changes require a new format version and compatibility review, not silent reinterpretation of stored hashes.

## Reproduce verification

The toolchain is pinned to Rust **1.99.1**, edition **2024**. Direct dependencies are pinned and the workspace lockfile is committed. Existing CI discovers the manifests and runs Rust checks; its required aggregate must see the Rust job succeed. No workflow weakening or merge-policy expansion is needed.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
python3 .github/scripts/ci.py inventory
python3 .github/scripts/ci.py repository
python3 .github/scripts/ci.py rust
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
```

Validation tests include a deterministic seeded graph/permutation harness and deep/large fixtures. Use `cargo test -p rolvya-plan-validator --test validation -- --nocapture` for test output; test timings are observations, not production throughput or memory guarantees. Test names and executed counts are recorded in the PR and [project status](../project/status.md).

T-001–004, PA-01–02 and structural PA-07 apply to this boundary. T-005/PA-05 cover only static/runtime identity separation; persisted retry/fallback, transactional group aggregation, activation idempotency and storage mutation guards remain unimplemented. Validation has no provider side effects (the pure-domain PA-09 boundary).

Next: R03/R05 SQLite persistence and durable activation, followed by R04/R06 hard-budget transactional admission. No scheduler, authentication, secret broker, provider, frontend, deployment or billable integration is implemented by this slice. Initial source/manifests remain unfamiliar/sensitive under the existing PR policy and require Owner manual review; tests do not supply human approval.
