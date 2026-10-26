# R07 deterministic fake worker contracts

Status: **Implemented and tested in-process simulation, not an authenticated remote service.** Started from confirmed merged [PR #16](https://github.com/ygtdmr/rolvya/pull/16), main `52f11c8d983e0177888b860d7de764b2190e740f`; its tree matches verified R06 head `c87018a`. R01–R06 remain the existing authority. Branch: `codex/fake-worker-contracts`.

The R07 `SimulationController` remains an ephemeral simulation. [R08 durable worker control](durable-worker-recovery.md) now supplies the SQLite journal, dispatcher, authoritative renewal and recovery using its shared transition logic; the R07 limitations below describe the original slice.

Binding direction: [worker-initiated HTTPS ADR 0006](../decisions/0006-worker-communication.md), [group-only ADR 0009](../decisions/0009-parent-aggregation.md), [hard budget ADR 0002](../decisions/0002-hard-budget-approval.md), [bounded-or-stop ADR 0010](../decisions/0010-inflight-budget.md), [atomic admission ADR 0031](../decisions/0031-atomic-task-budget-admission.md) and [worker timing policy](../architecture/worker-authentication-policy.md). See [task packages](initial-implementation-task-packages.md), [engineering defaults](../project/implementation-defaults-and-release-gates.md) and [R06 implementation](atomic-scheduler-admission.md). No architecture choice is reopened.

## Modules and executable interfaces

`rolvya_domain::worker` defines protocol version 1, validated worker/message/capability IDs, SHA-256 digests, bounded model names, declared capabilities, full attempt binding, execution profile, lifecycle messages, failure classes, lease offers, stop instructions and observation receipts. It depends on neither SQLite nor a cloud provider. The initial wire format is implemented data, not a claim that the earlier illustrative draft is a deployed HTTP API.

Every worker message binds project, plan/version, task, attempt, worker ID, fencing epoch, plan fingerprint and original context hash. Stable message ID plus sequence identifies an observation within that scope. Controller replies correlate to the message and retain the same binding. Registration is **attempt-bound capability declaration after a committed dispatch**, not a general provider enrollment registry.

`rolvya_persistence_sqlite::worker_simulation` implements:

- Bounded worker/controller JSON encode/decode functions. Unsupported versions, unknown fields/operations, malformed UTF-8/JSON, recursion, invalid IDs/hashes, oversized messages and invalid numeric fields reject with structured errors. No error echoes raw input.
- `SimulationController::attach`: consumes the existing non-deserializable R06 `DispatchPermit`, verifies the real active leaf, immutable assignment, current lease, committed claim and four-category reservation, then retains only compact profile/scope metadata. Arbitrary worker-supplied lease fields cannot create a session.
- `receive`: rechecks a separate trusted peer/capability verifier, complete scope/hash binding, current R06 lease and conservative reservation coverage before accepting ordinary messages. Unknown financial exposure, released funds, expiry, fencing or revoked authority fails closed. It never calls budget approval, graph mutation, Git publishing or authoritative result acceptance.
- `cancel`: a separate **trusted controller** operation invokes R06 fencing through the existing scheduler policy, returns a typed stop instruction and preserves holds. No corresponding privileged worker command exists. Worker stop acknowledgement is only an observation, not provider termination or final billing.
- `FakeWorker::script`: reproducible registration, claim/lease acceptance, heartbeat, progress and outcome events. Success creates bounded synthetic artifact bytes and their actual SHA-256 hash; failure reports controlled/transient/permanent/cancelled classes. Behaviors also include disconnect, delay, duplicates, out-of-order delivery, malformed bytes, expired/stale leases, missing/invalid capability evidence, project/attempt/context/identity attacks and result-after-cancellation.
- `DeterministicClock` and `EventQueue`: nondecreasing injected time, stable FIFO order at equal timestamps, explicit event scheduling and a finite lifetime event budget. The R07 core performs no sleeps, network access, process execution or background polling.

The SQLite adapter adds a direct reference to the workspace's already pinned `sha2` solely for real synthetic artifact hashes. Cargo.lock adds that dependency edge; no package/version upgrades or new resolved packages occur. R06 source, ledger business rules and migrations are unchanged. No R06 correctness defect required a fix.

## Lifecycle and authority separation

```text
R06 admitted/reserved -> committed dispatch claim -> attached simulation
  -> registered declarations -> worker claim / lease offer -> lease acceptance
  -> heartbeat/progress observations -> result received OR failure/disconnect
  -> independent validation is a separate trusted R06 action
```

`ResultReceived` is a simulation phase, never authoritative task success. A reported successful manifest hash is untrusted. Actual artifact content, scope/provenance and acceptance criteria must be checked independently before the existing `accept_task` boundary is called. The tests deliberately inspect a real synthetic artifact separately and then call that validator boundary explicitly; there is no automatic validation from a worker result or a full R09 orchestration loop.

Worker failures are observations. A transient label does not authorize retry, fence another attempt or release a hold. After failure/disconnect the protocol session accepts no new execution outcome. R08 must implement durable recovery and reconcile external effects before creating any newly budgeted attempt through R06. Validated sibling results remain intact.

Messages use strict increasing sequences; out-of-order messages do not consume a receipt or advance state. A duplicate accepted message returns its original in-memory receipt only after current authority checks; changed content under the same ID conflicts. Progress counters cannot decrease or change their declared total. Cached healthy stop queries cannot override later expiry/fencing/risk. Invalid/rejected input does not grow the receipt journal.

Stop queries may disclose a scoped stop instruction after expiry/fencing. A matching stop acknowledgement is allowed for that retired scope after peer verification; it grants no renewed execution authority and changes no financial state. Other late messages and stale epochs reject. Trusted cancellation uses R06's exact lease identity and monotonic fence, preserving the original reservation and frozen plan.

## Authentication and timing boundaries

`WorkerAuthority` is a mandatory `Send + Sync` verifier, separate from wire declarations. The routing `PeerEvidence` reference comes from a trusted transport/test-driver boundary and is **not** itself a credential. A real implementation must verify channel identity, project/attempt/action binding, capability evidence and revocation. A worker ID or a JSON `trusted` flag is never proof. `UnimplementedAuthentication` rejects every operation. Tests use explicitly scoped synthetic grants, not real authenticated identities, bearer tokens or verified provider limits.

No public HTTP endpoint, TLS service, production worker enrollment, provider attestation, Secret Broker access or credential issuance is implemented. The contracts can be carried by the approved future worker-initiated authenticated HTTPS transport. They contain no AWS-specific logic. The rollout remains synthetic workers, AWS ECS Fargate, Cloud Run Jobs, then enrolled remote VPS; no live adapter is added.

The approved **900-second token lifetime, 30-second heartbeat interval and 90-second lease duration** remain distinct advertised defaults. No token is minted. Heartbeat acknowledgements record observation only: the actual R06 SQLite lease deadline is unchanged and still capped by its admitted capability. Tests at 30/60/89/90 seconds prove that a heartbeat or token-default field cannot renew an expired lease. Real renewal is R08 work.

## Resource and data limits

Frames are limited to **8,192 bytes before JSON parsing**. Models are limited to 16 bounded names; effort/access-mode collections reject duplicates/invalid lengths. JSON recursion uses the parser's bounded default. Task context stays in existing immutable storage and is referenced by hash; the simulation never duplicates full prompts or logs raw task context. Progress uses structured codes, not free-form executable/model instructions. Declared model debug output and artifact debug output omit raw values/bytes.

Default simulation limits are 16 sessions, 64 accepted messages per session and 512 scheduled events, with validated hard maxima 128/256/4,096. These are configurable simulation memory/backpressure limits, **not** a product worker-concurrency policy. Queue insertion and parsing fail before exceeding bounds; a drained queue does not reset its lifetime budget. There is no unbounded retry/busy loop. Future network body buffering, rate limits and operational load control still need transport integration.

A single debug Cloud observation processed **24 independently admitted workers / 144 events / 69,404 queued wire bytes** in **195.809 ms** for the event-processing portion. All workers produced received outcomes, but zero tasks were automatically accepted; holds totaled the exact synthetic 168 minor units. This is an environment-specific observation, not a CPU/RSS profile, 2–4 GiB deployment proof, p95 latency or SLO.

## Verification and scope of evidence

**175 workspace Rust tests pass**, including eight new domain protocol tests and 32 new simulation tests. The existing eight migration replay tests still run separately through Rust CI; no SQL migration is added. Test fixtures use real R06 admission/dispatch/fence and real R04 budgets in disposable SQLite, scoped synthetic identity/capability verifiers and deterministic clocks. Tests prove budget/hold/topology/count/hash preservation and no result self-acceptance, alongside the full configurable failure matrix.

| Matrix case | R07 evidence / remaining boundary |
| --- | --- |
| T-013 | Every message rechecks R06 lease/fence; stale epochs and results after cancellation/fencing reject. Durable message replay/renewal and real tokens remain R08/security work. |
| T-019 | Independently validated A stays succeeded when B actually reports permanent failure within its lease. Untrusted worker success alone leaves task running. |
| T-020 | Fake worker has no SQLite/Owner/Git handle; controller binds only consumed committed permits and rejects forged identities, known other attempts/projects and unsupported authority commands. OS sandbox/remote credential isolation is not tested. |
| T-021 | Structured worker/model-like input cannot become budget, graph, acceptance or protected Git authority. Full real tool/model prompt-injection resistance is not claimed. |
| T-025 | Received leaf output does not satisfy group validation; independent child/group acceptance remains the existing R06 gate. No full R09 DAG run is claimed. |
| T-026 | Group/unplanned task claims reject and no additional attempts or DAG nodes are created by workers. |

RED→GREEN observations include unsupported protocol acceptance, replaying an outdated healthy stop reply after expiry and backwards progress under a newer sequence. Other negative cases cover malformed/oversized payloads, unsupported schemas, identity revocation during replay, incompatible/missing capability proof, cross-scope access, incorrect hashes, changed idempotency payloads, stale lease offers, fake failure-to-success conversion, unknown billing and released coverage. Resource tests cover FIFO order, event/journal caps, time regression and schedule overflow.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
python3 .github/scripts/ci.py rust
python3 .github/scripts/ci.py inventory
python3 .github/scripts/ci.py repository
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
cargo test -p rolvya-persistence-sqlite --test worker_simulation multiple_independent_fake_workers --locked -- --nocapture
```

Graphify query/explain/path guided source investigation and AST-only `graphify update .` refreshes source relationships. Its SQL parser is unavailable (`tree_sitter_sql` missing); existing migrations are executed directly. Generated graph/build/database artifacts remain ignored. No workflow, protection, permission or automatic-merge policy changes; this sensitive worker-boundary PR is for Owner manual review. **Zero live provider/model spending, live workers, infrastructure or production credentials were used.** No GitHub issues or merges are created.

## Next: R08

Protocol IDs, scoped observations, receipts, clock/verifier interfaces and one-time R06 operation identity are the R08 boundary. Session journals and phases are **ephemeral**. A new controller cannot recover a session merely from a worker ID; restart rejects frames, and R06 still rejects reissuing the dispatch claim. Durable deduplication/acknowledgements, receipt/state transactions, lease renewal, stale-worker recovery, outbox/provider replay and crash-safe reconciliation remain unimplemented. No exactly-once external billing or real cloud cancellation is claimed. R09 end-to-end orchestration follows R08; neither is silently included here.
