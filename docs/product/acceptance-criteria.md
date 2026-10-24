---
title: Acceptance Criteria and Traceability
status: Proposed
category: product
last-reviewed: 2026-10-21
tags: [acceptance, requirements, tests]
---

# Acceptance criteria and traceability

This is a **future implementation test contract**, not evidence tests have run. Requirement identifiers refer to [product requirements](requirements.md).

| Test ID | Scenario and steps | Pass condition | Requirements |
| --- | --- | --- | --- |
| AC-001 | Submit a DAG with A→B→A or missing predecessor ID. | Entire plan rejected; no runnable jobs created. | PR-001 |
| AC-002 | Activate a valid DAG, cause a worker failure and then retry with another model. | Same task and graph identities retained; new attempt and audit record created. | PR-002, PR-005 |
| AC-003 | Have worker submit request to create a subtask. | Denied without modifying active DAG. | PR-002, PR-003 |
| AC-004 | Configure A prerequisite of C; B independent. | B may run while C remains blocked until A validates. | PR-004 |
| AC-005 | Ready tasks write overlapping remote Git files. | Integration isolation/conflict policy prevents silent overwrite. | PR-004, PR-007 |
| AC-006 | Run control plane on resource-constrained remote VPS with remote worker. | Remote dispatch and full state recovery work within a benchmark envelope to be approved. | PR-006, PR-012 |
| AC-007 | Inspect connector configuration and invoke repository operations. | Only remote endpoints are supported; no personal-computer Git paths or Local Sync. | PR-007 |
| AC-008 | Concurrently admit two tasks whose combined exposure exceeds available budget. | Transaction admits at most fully authorized exposure; no percent overrun. | PR-008 |
| AC-009 | Deny proposed budget increase, then approve a separate valid increase. | Denial leaves admission paused; only authenticated approval changes ceiling and enables reevaluation. | PR-008 |
| AC-010 | Consume budget mid-project. | Previously completed outputs remain available; newly billable tasks pause. | PR-008 |
| AC-011 | Generate cost estimate and accounting report. | AI API, compute, storage, transfer separated; optimistic/expected/pessimistic estimates distinct from actual spend. | PR-009 |
| AC-012 | Plan mixed-model work with access modes. | Task-level model/effort/mode retained and unauthorized modes rejected. | PR-005, PR-010 |
| AC-013 | Crash after provider writes but before local acknowledgement. | Reconciliation/idempotency prevents duplicate write and double reservation. | PR-008, PR-012 |
| AC-014 | Attempt to leak credentials via logs or model output. | Sensitive values redacted; access rejected and security event tracked. | PR-013 |
| AC-015 | Inspect docs in GitHub and repository-root Obsidian Vault. | Existing links resolve; proposed versus implemented status is explicit. | PR-013 |
| AC-016 | Inspect/build all Rolvya-owned backend services. | Backend services and application logic are Rust-only; no unapproved parallel Go/Python/Node backend. External user project tools remain permitted. | PR-014 |
| AC-017 | Initialize authoritative persistence and concurrently submit budget reservations, then crash/recover it. | SQLite is the only authoritative engine; transactional budget integrity, foreign keys, approved-ceiling protection and restored successful artifacts hold. | PR-008, PR-012, PR-015 |
| AC-023 | Define group G containing executable A and B, with explicit prerequisite G → C. Validate A only. | G remains incomplete and C blocked; no worker attempts scheduled for G. | PR-001, PR-004, PR-016 |
| AC-024 | Independently validate required A/B and group acceptance, then replay child completion. | Exactly one group success transition; C becomes eligible; immutable plan hash unchanged. | PR-002, PR-016 |
| AC-025 | New task exceeds headroom while another running attempt has a verifiable reserved provider bound. | New admission denied; existing attempt remains within its already-authorized bound. | PR-008, PR-017 |
| AC-026 | Running worker loses enforceable spending cap; late invoice arrives after cancellation request. | Stop-on-risk/fencing/cancellation and reconciliation recorded, cost uncertainty visible; no automatic ceiling change. | PR-008, PR-009, PR-017 |

## Approved transport, GitHub and multi-provider acceptance criteria

| ID | Scenario | Pass condition | Requirement |
| --- | --- | --- | --- |
| AC-027 | Enrolled worker communicates via HTTPS claim/heartbeat/result; sends stale or forged epoch | Authenticated, fenced result handling; no unreserved/unauthorized task success | PR-018 |
| AC-028 | Trusted Rust publisher uses GitHub App and worker requests protected branch mutation | Allowed ref policy and independent merge guard deny attempt; no broad App token in model-visible environment | PR-019 |
| AC-029 | Two authorized fake provider adapters run independent executable leaves | Same immutable DAG, Rust scheduler, HTTPS worker protocol and SQLite budget accounting | PR-020 |
| AC-030 | Provider A fails, new attempt chooses allowed B; B lacks approved cost or permission in negative case | Same task ID, new attempt/fence/reservation only when authorized; unsafe fallback denied | PR-008, PR-017, PR-020 |

The specific cloud vendors/regions for the first release remain unselected. These are **future acceptance tests**, not executed results; see [integration test matrix](../development/remote-integration-test-matrix.md).

## Additional provider and security acceptance cases

| Test ID | Scenario | Pass condition |
| --- | --- | --- |
| AC-018 | Cloud provider automatically restarts a failed worker container | Retry is disabled or financially accounted for as an authorized Rolvya attempt; no silent extra paid execution |
| AC-019 | GitHub token has Contents write but worker attempts protected-branch write | Worker cannot bypass trusted integration control or approved protected-branch rules |
| AC-020 | Owner dismisses an expense alert without approving a new ceiling | Approved ceiling and admission pause remain unchanged |
| AC-021 | Provider invoice is delayed or cannot be reconciled | Cost remains marked uncertain; new unsafe billable work is not admitted |
| AC-022 | Worker impersonates another project or submits stale lease epoch | Authorization fails and audit logs capture a sanitized denial |

Related designs: [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [worker transport](../architecture/worker-transport-design.md), [identity](../architecture/identity-authorization.md) and [GitHub App isolation](../architecture/github-app-integration-design.md).

## Historical requirement preservation acceptance cases (future tests)

| Test ID | Scenario | Pass condition | Existing requirement / decision |
| --- | --- | --- | --- |
| AC-031 | User configures subscription-only Claude CLI, but remote third-party authorization cannot be verified | Block unattended paid worker; no credential spoofing or silent API fallback | PR-010, [model entitlement](../architecture/model-access-entitlement-contract.md) |
| AC-032 | Scheduler ranks high-priority task lacking capability/cost/region permission above safe task | Hard eligibility gates win; unsafe task blocked and candidate reasons recorded | PR-004, PR-006, PR-008, PR-020 |
| AC-033 | A retry instruction refers to historical 1/3/10-minute and three-attempt graphic | It is **not** silently activated as a universal default; requires explicit policy/config approval | PR-002, PR-005, [retry design](../architecture/retry-escalation-policy.md) |
| AC-034 | Synthetic 40-task estimate has four cost classes and three scenarios | Arithmetic matches documented $2.23 / $4.15 / $8.96 **hypothetical** figures; distinct hard bound and explicit $5 ceiling cannot be silently raised | PR-008, PR-009, [worked example](../architecture/cost-scenario-worked-examples.md) |
| AC-035 | Compress planner/worker context to save tokens | Task goals, security/budget constraints, required upstream validated artifacts and acceptance evidence remain preserved | PR-003, PR-005, PR-010, PR-013 |
| AC-036 | Branding/license consistency check | README slogan non-repeated, final no-gold wordless logo not replaced by old gold design, code license stays MIT (SPDX `MIT`) with the required copyright and permission notice | [Brand history](../product/brand-history-license-governance.md), [LICENSE](../../LICENSE) |

These are **specified, not executed**, and tests depending on as-yet-unapproved CLI subscription use or scheduler parameters remain **blocked pending entitlement/policy evidence**, not passed.

## Approved three-role project permissions — future acceptance tests

| Test ID | Scenario | Required result | Requirement |
| --- | --- | --- | --- |
| AC-037 | An Editor starts a run already approved by the Owner, with valid remaining project budget | Allowed only within the unchanged approved preflight plan, model/provider/security limits and new atomic cost reservation | PR-008, PR-021 |
| AC-038 | An Editor or Viewer requests higher budget, unapproved provider/Git scope, or a new immutable plan | Denied for lack of Owner authorization; no billable expansion or privilege escalation | PR-008, PR-019, PR-021 |
| AC-039 | Viewer requests only authorized status/results, then attempts to launch a worker or inspect raw secrets | Scoped read succeeds; privileged actions and secret access are rejected | PR-021 |
| AC-040 | Editor has project A access but not project B; revoked role attempts a delayed queued action | Project B read/launch denied; queued action revalidates membership and refuses when revoked | PR-013, PR-021 |

These are **specified, not executed**; the exact login/session provider and role storage implementation still require engineering decisions.

## Human web sessions and token theft defense — proposed future tests

| Test ID | Scenario | Expected result | Scope |
| --- | --- | --- | --- |
| AC-041 | Active authorized web user renews a server-managed session | Low-risk interaction proceeds without unnecessary login, while server controls expiration and revocation | ADR 0027, [proposed token contract](../architecture/session-token-rotation-and-theft-response.md) |
| AC-042 | No human interaction for 7 days, but background workers/automatic polling continue | Background requests never extend web-user idle lifetime; session expires under policy while independently authorized workers remain governed by their own credentials | ADR 0027 |
| AC-043 | Stolen old session handle is replayed after rotation or device revocation | Stale/revoked handle denied, no restored privileges or extra budget | ADR 0027, proposed token contract |
| AC-044 | An optionally enabled federated OIDC provider reports refresh-token reuse when supported | Affected grant/session family blocked and incident recorded; this is conditional IdP coverage, not a dependency of initial password/email sign-in | ADR 0025, proposed token contract |
| AC-045 | Concurrent tab session renewals or lost response | Atomic state cannot resurrect stale token generation; legitimate session handling is defined and testable | Proposed token contract |
| AC-046 | Owner requests budget increase through normal renewed session without recent verified strong step-up | Critical action denied pending strong reauthentication; UI continuity alone cannot approve spending | ADRs 0002, 0025, 0027 |
| AC-047 | User revokes an individual device session or all web sessions | Server-side revocation effective for selected sessions, audited, without indiscriminately cancelling unrelated preapproved worker attempts | ADR 0027 |
| AC-048 | Product scope is assessed for web v1 | No implicit commitment to mobile app or Jarvis-like voice assistant; both explicitly remain optional future concepts | [Product scope](scope.md), ADR 0027 |

These are *specified future tests, not executed*. The proposed implementation mechanisms are not owner-approved by documenting these scenarios.

## Owner-controlled project invitations — future acceptance cases

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-049 | Editor or Viewer requests Editor or Viewer invite | Rejected: **only Owner** may invite; role checks server-side | PR-021, PR-023 |
| AC-050 | Owner invites Editor to project A, token is opened without sign-in | Token alone cannot confer access; verified identity through an enabled primary login (including their voluntarily enabled TOTP, if any), durable account binding and matching recipient constraints | PR-023 |
| AC-051 | One-use invitation is redeemed concurrently twice in SQLite | At most one atomic successful redemption and membership grant | PR-012, PR-023 |
| AC-052 | Invitation is redeemed at/after 24 hours from issuance, revoked/tampered, or addressed to another authenticated account | No membership grant and no cross-project data leakage | PR-013, PR-023 |
| AC-053 | Owner attempts transfer without recent verified strong step-up or target independent authenticated human identity/explicit acceptance; also simulate failed final commit | Denied/incomplete until both approve; original Owner remains authorized until atomic successful handover; no ownerless project | PR-021, PR-023 |
| AC-054 | New Editor after successful invite attempts to raise budget, expand Git/provider scope or activate unapproved DAG | Denied: membership never overrides prior Owner approvals or frozen execution plan | PR-008, PR-019, PR-023 |

All scenarios are **specified only**. No end-to-end implementation or validation has been performed.

## Multiple Owners — specified future tests

| Test ID | Scenario | Expected result | Requirements |
| --- | --- | --- | --- |
| AC-055 | Add a second Owner with authenticated current Owner step-up and prospective Owner independent permitted login/explicit acceptance | Both independently verified identities hold scoped Owner membership; no inherited session or role confusion | PR-021, PR-023 |
| AC-056 | Two concurrent removals/demotions could leave project with zero active Owners | SQLite authorization transaction rejects the operation that would remove the final Owner; retain at least one verified Owner | PR-021, PR-023 |
| AC-057 | Co-Owners perform separate budget management actions | Each must authenticate as themselves and explicitly approve permitted changes; no implicit authority for AI agents, no hidden override of hard budget | PR-008, PR-023 |

Tests specified, **not implemented or executed**.

## Multiple Owners — budget approval notification (specified future tests)

| Test ID | Scenario | Expected result | Requirements |
| --- | --- | --- | --- |
| AC-058 | Project has two authenticated Owners; Owner A explicitly raises budget after verified strong step-up; Owner B has not approved | Exactly one atomic auditable increase is applied; no unanimous co-Owner approval required; admission remains bounded by the new ceiling | PR-008, PR-021 |
| AC-059 | Budget increase from Owner A succeeds; Owner B's in-app delivery temporarily fails and Owner B disabled email | Committed increase remains valid; mandatory in-app notice is durably queued/retried and auditable; no email is sent against preference; no duplicate spending or second approval | PR-008, PR-021 |
| AC-060 | Editor/Viewer/agent tries to authorize increase; two Owners race with stale ceiling versions | Unauthorized changes rejected, stale version rechecked; no lost update, duplicate increment or automatic overrun | PR-008, PR-021 |

These are **specified only**, not implemented or executed.

| AC-061 | Owner A increases ceiling, Owner B allows email and Owner C disables it | Both B and C receive in-app notifications; only B is eligible for email; notices never act as approvals | PR-008, PR-021 |

## Project Activity Log — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-062 | Owner increases budget; worker task fails and retries; Editor changes permitted project setting | Activity timeline shows correct chronological project events, verified human/machine attribution and separate approval/attempt outcomes | PR-024 |
| AC-063 | Owner/Editor/Viewer submits activity-history edit or delete | Rust denies the mutation for all ordinary roles; a correction can only be appended as a new event | PR-021, PR-024 |
| AC-064 | User from project A queries project B events; Viewer requests sensitive unauthorized event details | Cross-project read denied; Viewer sees only permitted read-only subset | PR-021, PR-024 |
| AC-065 | Worker stdout or incoming event contains a token; renderer lists timeline details | Confidential content is redacted/excluded from event store, UI, search and notifications | PR-013, PR-024 |
| AC-066 | Crash or repeated callback occurs immediately after budget or membership commit | Recovery yields one truthful durable activity record per effective state change, without authorizing duplicate spending | PR-008, PR-012, PR-024 |

These are **specified but not executed**; no deployed Activity Log or test pass is claimed.

## Activity Log access window and archival — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-067 | Authorized project member views activity events aged 1–90 days | Permitted records remain readily accessible in the project panel with scoped role authorization | PR-024 |
| AC-068 | Normal event ages beyond 90-day active window and archived storage is enabled | Compression/archive preserves provenance, integrity, scoped retrieval and recovery without treating age 90 days as mandatory deletion | PR-024 |
| AC-069 | Critical budget/security approval event ages beyond 90 days | Preserve for at least 3 years total subject to law; keep secrets excluded | PR-008, PR-024 |

These tests are **specified, not implemented or executed**. Total retention windows are Approved: 1 year routine history and at least 3 years critical financial/security records, subject to law. Archive/deletion mechanics remain open.

| AC-070 | Routine event is older than one year; critical security event is within three-year retention window | Enforce lawful routine lifecycle and preserve critical event; exported and backup copies follow approved retention controls | PR-024 |

## Cryptographic audit integrity — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-071 | Modify or omit a historical budget/security event in active or archived log storage | Verification against trusted integrity reference flags alteration/sequence gap and marks affected history unverified | PR-024 |
| AC-072 | Replace an entire locally hashed chain and its unprotected chain head | Independent protected integrity reference prevents falsely reporting the replacement as verified | PR-024 |
| AC-073 | Export/archive/restore or lawfully expire records | Verify surviving history and authenticated lifecycle transitions without treating authorized expiration as invisible tampering | PR-024 |

These tests are **specified, not executed**; integrity design and protected key/checkpoint mechanics remain open.

| AC-074 | Configure independent audit checkpoint adapter with S3-compatible and a distinct non-S3-capable fake provider | Both satisfy the same verification/retrieval contract without binding audit logic to S3-specific APIs | PR-024 |
| AC-075 | SQLite event history is rewritten while protected checkpoint storage is unchanged; selected provider does not support retention/versioning | Tampering is detected; unavailable protection is surfaced instead of claiming immutable storage | PR-024 |

| AC-076 | Deployment administrator allows storage target A for project, denies B; Owner selects destination; Editor or worker tries to add endpoint B | Owner may select only A, and unauthorized endpoints/writes are rejected in Rust before secrets or data leave the deployment | PR-013, PR-021, PR-024 |
| AC-077 | Administrator disables configured checkpoint target or Owner switches among allowed targets | Data cannot silently migrate to forbidden target; prior checkpoints remain verifiable, and unavailable protections/destinations are surfaced safely | PR-024 |

## Storage switch and explicit historical migration — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-078 | Owner changes the configured destination to a newly allowed provider with valid access, while old archives/checkpoints exist | Validate new target and allow only future eligible writes; **do not copy, move, or delete any old archive/checkpoint** | PR-024 |
| AC-079 | Target is inaccessible, fails integrity safeguards, or ceases to be allowlisted | Cutover refused, no silent fallback or migration; old data/checkpoints retain truthful recorded locations and recovery status | PR-024 |
| AC-080 | Owner explicitly approves historical-data migration after reviewing scope and budget; transfer partially fails | Budget/security gates remain enforced; retries are safe; migrated copies independently verified before acceptance; source not silently deleted | PR-008, PR-024 |
| AC-081 | Editor, worker, or background change event tries to migrate stored history without a separately approved Owner operation | Migration and billable data transfer denied, with auditable decision | PR-021, PR-024 |

These tests are **specified, not implemented or executed**.

## Post-migration original-copy protection — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-082 | Owner-approved migration completes and independent destination integrity and retrieval verification succeeds | Old-provider archives and integrity checkpoints remain intact; no automatic delete, purge or inferred cleanup approval | PR-024 |
| AC-083 | Strongly reauthenticated project Owner explicitly approves a separately scoped source cleanup after destination checks and applicable retention/hold checks pass | Only approved objects are eligible for bounded auditable deletion under administrator-allowed provider rules; report confirmed and uncertain outcomes truthfully with protected verification evidence | PR-021, PR-024 |
| AC-084 | Cleanup is requested when destination completeness/checkpoint integrity fails, mandatory retention has not elapsed, a legal hold applies, or deletion would destroy the final trusted reference | Cleanup denied or deferred; source and trusted anchors preserved; attempted action and reason auditable without secrets | PR-024 |
| AC-085 | Editor, Viewer, worker, autonomous agent or migration-completion callback attempts deletion without distinct strongly authenticated Owner approval | Deletion denied; no implicit permission from target selection or migration success; denial and source preservation auditable | PR-021, PR-024 |

These are **specified future tests, not implemented or executed**.

## Resumable historical migration and recovery — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-086 | Migration manifest contains 1,000 source objects; connectivity fails after 800 entries have independent, durable destination verification | Resume reuses still-valid verified evidence for 800 and processes/re-verifies remaining 200; no blind full recopy or source deletion; incomplete state visible until full manifest verifies | PR-024 |
| AC-087 | Destination provider accepts upload but controller crashes before persisting acknowledgement | Reconcile staged object/version and external provider costs before retry; avoid double logical completion or overwriting verified object; unknown billing not treated as zero | PR-008, PR-024 |
| AC-088 | Provider acknowledges a copied archive, but independent read-back/hash/version check fails | Object remains unverified; migration cannot complete, old source remains intact, discrepancy audited | PR-024 |
| AC-089 | Historical source object changes mid-operation or a consistent snapshot/version is unavailable | Detect mismatch; block or revalidate safely; do not silently import a new source version or widen authorized manifest | PR-021, PR-024 |
| AC-090 | Project has insufficient approved budget for the next storage/transfer/verification attempt | No new billable work; preserve prior verified progress and source; UI reports budget pause without automatic ceiling increase | PR-008, PR-024 |
| AC-091 | An in-flight transfer loses enforceable billing bound; later provider charges arrive | Apply stop-on-risk/fence where feasible, reconcile unknown and late charges, refuse unsafe new admission; do not claim guaranteed refunds or extra headroom | PR-008, PR-024 |
| AC-092 | Authorized Owner explicitly increases approved budget after pause; prior manifest and security scope remain valid | Re-run eligibility and provider/cost reconciliation before resuming incomplete work; no requirement to discard verified objects or expand scope | PR-008, PR-021, PR-024 |
| AC-093 | Editor, Viewer, worker, automation or stale migration callback attempts resume with altered object set, destination or financial authority | Authorization and immutable scope checks reject change; no unauthorized data movement, deletion or spending | PR-008, PR-021, PR-024 |
| AC-094 | Target is de-allowlisted, region changed or required capability revoked during pause | Resume denied and old source preserved; do not silently fall back to other target; record accurate partial-copy locations and security cause | PR-013, PR-024 |
| AC-095 | Archive payloads copied, but protected audit-checkpoint trust continuity cannot be independently verified | Whole-manifest verified completion refused; original source and independent trust anchors survive | PR-024 |
| AC-096 | Owner cancels migration during a remotely charged in-flight transfer | Prevent new work, fence/cancel when enforceable, reconcile possible late side effects and charges; keep original source and accurate partial destination status | PR-008, PR-021, PR-024 |
| AC-097 | Old lease generation and duplicate callbacks race with resumed migration after restart | Stale generation cannot alter verified journal, expand scope or falsely mark operation completed; replay remains idempotent at logical-state level | PR-012, PR-024 |
| AC-098 | Some objects are staged but not independently verified, followed by nonrecoverable provider failure | Preserve original sources, trusted checkpoints and durable proven work; show partial/blocked status, safe recovery options and attributable costs; never auto-delete | PR-024 |

These are **future requirements, not implemented or executed tests**. Exact provider calls, retry counts, chunk sizes and SQLite schema remain proposed in [the migration state machine](../architecture/storage-migration-state-machine.md).

## Bounded transient retry and backoff — specified future tests

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-099 | Allowed migration encounters a classified transient network timeout, then recovers within 4 total attempts (initial plus up to 3 automatic retries) | Eligible retries use approved 5/10/20-second base waits before retries 1/2/3, with bounded jitter/provider constraints; only unverified affected work proceeds; source, verified objects and hard budget/security gates remain protected | PR-008, PR-024 |
| AC-100 | Provider returns throttling/rate-limit metadata with a valid `Retry-After` longer than the scheduled base delay plus jitter, or a confirmed temporary 5xx outage | Honor the longer provider-required minimum wait, preserving approved 5/10/20-second baseline and finite attempt ceiling; prevent hot-loop requests/retry storms | PR-024 |
| AC-101 | A logical storage-transfer operation fails on all 4 permitted attempts (initial attempt and 3 automatic retries) | The fifth total attempt is not scheduled automatically; durable blocked/review state, progress, cause and cost uncertainty remain visible; restart does not reset exhaustion | PR-024 |
| AC-102 | Upload response is lost after the provider may already have committed a chargeable write | Reconcile remote object/version and potential billing first; no blind duplicate upload or falsely zero charges | PR-008, PR-024 |
| AC-103 | Independently checked destination object has content/hash mismatch or protected checkpoint verification fails | No automatic retry treating integrity corruption as transient; no false verified completion; source and trusted anchors retained | PR-024 |
| AC-104 | Migration retry would require unauthorized endpoint/region, revoked permission or violate retention/legal hold | Retry denied without implicit provider fallback, privilege expansion, source deletion or billable transfer | PR-013, PR-021, PR-024 |
| AC-105 | Transient transfer error occurs but available approved headroom is insufficient or provider-side spending bound becomes uncertain | No new billable automatic retry; pause and preserve verified progress and source; explicit Owner ceiling approval required only when an increase is needed | PR-008, PR-024 |
| AC-106 | Controller restarts during backoff or duplicate/stale provider callbacks race with an eligible retry | Persist retry count, next eligibility and fencing; stale callbacks cannot schedule excess attempts or erase verified progress | PR-012, PR-024 |
| AC-107 | Destination verification read temporarily fails, then succeeds; the already staged object remains unchanged | Retry only the transient read within bounds, not a blind full upload; mark verified only after independent evidence succeeds | PR-024 |
| AC-108 | Migration contains multiple storage operations while executable coding-task retry default is 3 total attempts | Storage retry default is 4 total (1 initial + at most 3 retries), distinct from AI worker's 3 total; no implicit additional billable work | PR-008, PR-024 |
| AC-109 | A transient transfer operation completes attempts 1 through 4 unsuccessfully, then a delayed callback or restart requests another retry | Strict maximum of 4 total attempts; attempt 5 rejected, persisted counter and idempotent operation identity prevent restart/replay bypass; source and independently verified copies remain intact | PR-008, PR-012, PR-024 |
| AC-110 | A transient logical storage-transfer operation requires all approved retry opportunities and provider imposes no extra waiting | The base delays are 5 seconds before total attempt 2, 10 seconds before attempt 3 and 20 seconds before attempt 4, with small bounded jitter; delay schedule is not mistaken for cumulative timestamps, a worker retry policy or a fifth attempt | PR-024 |
| AC-111 | Provider requires a wait longer than the local jitter-adjusted 5/10/20-second retry delay | Retry is deferred until at least the valid provider-imposed wait; local jitter cannot override provider minimum, and the retry count/budget gates still apply | PR-008, PR-024 |
| AC-112 | Controller restarts while an object retry is scheduled with approved backoff/jitter and a provider cool-down | Durable attempt count and eligible-after deadline are recovered; no prematurely dispatched retry, clock-reset retry storm or extra fifth attempt; original source and verified progress survive | PR-012, PR-024 |

These are **specified future tests, not implemented or executed**. The **4-total-attempt storage retry default**, **5/10/20-second base delays**, **small bounded jitter**, and **priority of a longer provider-mandated wait** are Approved. Exact jitter magnitude/distribution, provider cool-down parsing, operation granularity, error mapping and scheduling implementation remain Proposed. See [storage retry design](../architecture/storage-migration-state-machine.md) and [ADR 0030](../decisions/0030-project-activity-audit-log.md).

## Human login and authenticator-app 2FA — specified future tests (ADR 0033 supersedes ADR 0032)

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-113 | User configures permitted password sign-in and registers/authenticates | Secure salted memory-hard password verifier; rate limits, safe reset and tenant role checks; no plaintext storage | ADR 0033 |
| AC-114 | Initial setup chooses zero primary login methods or TOTP-only | Refuse configuration: at least one of password or email verification must be enabled | ADR 0033 |
| AC-115 | User enters email for a one-time link or code | Successful verified, single-use unexpired challenge grants only permitted ordinary sign-in; rate limits and enumeration defenses apply | ADR 0033 |
| AC-116 | Expired, replayed, concurrent, tampered or brute-forced email challenge is submitted | Deny authentication; atomic one-time consumption, token confidentiality and safe redirect hold | ADR 0033 |
| AC-117 | Owner enables additional password or email verification method later | Require current Owner authority and recent strong reauthentication; audit change, preserve existing user access, and enforce secure method enrollment | ADR 0033 |
| AC-118 | User links password and email-only sign-in identities | Deny unsafe auto-link based on matching email alone; require verified control and preserve durable account identity | ADR 0033 |
| AC-119 | Owner requests high-risk budget or ownership change with a stale session and no fresh server-verified reauthentication | Reject until transaction-bound fresh verification, authorization and applicable risk controls; never require TOTP enrollment as a prerequisite or falsely label one-factor proof as MFA | ADRs 0025–0029, 0033 |

| AC-120 | Account owner enrolls authenticator-app TOTP and verifies the first valid code | Activation only after successful proof; encrypted secret, recovery controls and rate limiting | ADR 0033 |
| AC-121 | Replayed, expired or brute-forced TOTP code is submitted | Denied, with replay prevention and throttling; SMS never required | ADR 0033 |
| AC-122 | Non-Owner or AI agent attempts changing available login methods | Rejected server-side and recorded in audit log | ADR 0033 |
| AC-123 | Owner disables the last usable primary login method or strands account holders | Refused pending supported verified migration/recovery; no TOTP-only login configuration | ADR 0033 |
| AC-124 | TOTP device is lost and recovery is requested | Strict, auditable protected recovery with revocation, no hidden second-factor bypass | ADR 0033 |

| AC-125 | Password reset requested without configured or working outbound email | Do not claim a reset message was sent; display privacy-preserving **contact the instance administrator** guidance | ADR 0033 |
| AC-126 | Administrator-assisted recovery when outbound mail is absent | Independently verify identity, constrain and audit recovery, protect 2FA, revoke affected sessions; no arbitrary password disclosure or Owner privilege bypass | ADRs 0026, 0033 |
| AC-127 | Email-only primary login is configured while outbound mail is unavailable | Reject unsafe configuration and preserve an operational primary login method | ADR 0033 |

| AC-128 | An Owner who has not opted into TOTP requests a budget ceiling increase or ownership operation | No forced TOTP enrollment; require an approved recent non-TOTP verification route, explicit authorization, audit and risk safeguards | ADR 0033 |
| AC-129 | A user has voluntarily enabled authenticator-app TOTP | The account's enrolled second factor is respected in its protected sign-in/reauthentication flow, but unenrolled users have no compulsory 2FA | ADR 0033 |
| AC-130 | Admin/Owner policy attempts to make TOTP enrollment compulsory for an ordinary user or a high-risk Owner action | Reject mandatory enrollment; preserve user opt-in, fresh Owner reauthentication, risk safeguards and role/budget boundaries | ADR 0033 |

| AC-131 | Owner without enrolled TOTP submits a legitimate budget increase with fresh password proof | Do not require TOTP enrollment; reverify immutable action amount/project, current Owner role and risk checks; commit once with audit | Approved ADR 0034; implementation Proposed |
| AC-132 | Email-only Owner submits a critical approval with new, session/action-bound Magic Link/OTP | New proof must be one-use and transaction-bound, not reusable general permission or mislabeled MFA; fail safely when mail is unavailable | Approved ADR 0034; implementation Proposed |
| AC-133 | Pending privileged action is changed, replayed, or session/membership changes before final commit | Invalidate stale proof, deny altered effect, require fresh approval; protect atomic version/idempotency checks | Approved ADR 0034; implementation Proposed |
| AC-134 | Administrator or revoked Owner tries to approve another project's budget without membership | Reject despite valid login or deployment administration; do not confer Owner powers | ADRs 0024, 0029, 0034; implementation Proposed |
| AC-135 | User with voluntarily enrolled TOTP attempts an unsafe credential downgrade to bypass it during privileged approval | Preserve enrolled factor's protections and controlled recovery, without imposing enrollment on TOTP-free users | ADRs 0033, 0034; implementation Proposed |

**AC-131–135 now trace the Owner-approved ADR 0034 principle, but their physical implementation/tests, numeric timers, detailed threat thresholds and recovery algorithms remain Proposed. None has been run.**

| AC-136 | Critical Owner action authorization is used more than 5 minutes after fresh verified proof | Reject at/beyond expiration; a renewed browser session never extends action proof and a new challenge/approval is needed | ADR 0034 |
| AC-137 | Magic Link or email OTP is redeemed 10 minutes or more after issuance | Reject at/beyond expiry for login or transaction-bound reauthentication; never renew TTL on resend or page refresh of same challenge | ADR 0034 |
| AC-138 | A user submits incorrect email OTP values five times for one issued challenge, including concurrent attempts | Fifth failure exhausts challenge; later submission (even a correct code) is denied; enforce atomically and protect against restart/resend bypass | ADR 0034 |
| AC-139 | A successful email challenge or exact-action authorization is replayed, concurrently or after server restart | Reject replay; one successful consume/one effect at most even when token has not reached expiry | ADR 0034 |

| AC-140 | Owner submits a budget or security change using a credibly stolen/replayed human credential | Deny/hold affected high-risk action before commit, invalidate authorizations tied to stolen proof/session, record sanitized incident | ADR 0035 |
| AC-141 | Unrelated project/users and a second valid human session exist when one Owner operation is held | Do not globally pause those unaffected principals/projects; revoke the stolen affected human session as appropriate and preserve safe ordinary use | ADRs 0028, 0035 |
| AC-142 | Unrelated already-authorized worker continues with valid lease, isolated credentials, verified reservation and unchanged spending bound | No automatic cancellation from human suspicion alone; outputs/budget remain intact and no new permission/spend granted | ADRs 0010, 0031, 0035 |
| AC-143 | Human account incident also reveals compromised worker credential, revoked provider token or unenforceable budget | Apply independent affected worker/financial stop-on-risk and fencing despite default continuity of unrelated healthy workers | ADRs 0010, 0031, 0035 |
| AC-144 | New-IP-only signal is asserted as account compromise or held action is retried without new proof after review | No automatic account takeover verdict from signal alone; held action requires fresh transaction-bound authorization and a risk policy review | ADRs 0034–0035 |

**These acceptance scenarios implement future security requirements from ADRs 0033–0035; no authentication implementation or test has been executed.**

## Evidence rules

Each implementation claim needs source commit, repeatable test command, environment/version, result and known limitations. A proposed acceptance test is not a completed test. Automated checks should run in CI before merging behavior changes; security-critical behavior must include negative tests.

## Pending decisions

Performance thresholds, service reliability targets, supported worker providers, concrete authentication tests, timeout policy, UX/approval presentation and which tests gate initial release.

See [testing strategy](../development/testing.md), [project status](../project/status.md), and [decision register](../decisions/README.md).

## Multi-account CLI and API validation (future tests)

| AC-145 | Connect two distinct Claude Code accounts and one Codex account using separately authorized, provider-supported CLI sign-in | Each connection binds a distinct account/grant and private remote home; no access to a personal-computer CLI login is required | ADR 0038 |
| AC-146 | Run separate eligible account connections in parallel on remote workers | No accidental shared CLI credential, CLI history, model access account or filesystem; respect verified per-account concurrency/quota limits | ADRs 0037, 0038 |
| AC-147 | CLI session needs reconnect, terms cannot be verified, or subscription quota is exhausted | Block the affected connection; no silent account rotation, official client impersonation or paid API fallback | ADR 0038 |
| AC-148 | Disconnect or replace one provider account while another is running | Access revocation and credential rotation remain isolated; no other account's grant is overwritten or leaked | ADR 0038 |
| AC-149 | API-key access and CLI subscription account are configured for the same project | Exact selected mode and authorized connection are enforced; API billing, cloud compute, storage and transfer remain distinguishable | ADRs 0037, 0038 |

**AC-145–149 are proposed future validation tests for the approved ADR 0038 architecture. No runtime or provider-permission tests have been performed.**


## First-working provider rollout — future tests (ADRs 0040–0041)

| Test ID | Scenario | Pass condition | Requirement |
| --- | --- | --- | --- |
| AC-150 | Rust/SQLite fake providers exercise independent ready leaves and blocked dependencies under a low test budget | Validated frozen DAG, atomic budget holds, redacted audit, crash/replay-safe result fencing; no live cloud/model charges | ADRs 0040–0041, PR-028 |
| AC-151 | A first real, authorized AWS ECS Fargate worker completes one task before Cloud Run Jobs and remote VPS adapters exist | A correctly scoped/fenced, independently validated task can complete and reconcile authorized cloud costs; absence of other live adapters does not make the first working integration fail | ADR 0040, PR-028 |
| AC-152 | Add Google Cloud Run Jobs or an enrolled remote VPS after the AWS Fargate-first working version | Existing DAG/task IDs, independent credentials, Rust provider interface and SQLite budget reservation remain valid; no local-device agent or silent additional spending | ADRs 0011, 0039–0041, PR-028 |

**Specified for future verification, not implemented or run.**
