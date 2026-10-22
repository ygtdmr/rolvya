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
| AC-036 | Branding/license consistency check | README slogan non-repeated, final no-gold wordless logo not replaced by old gold design, code license stays GNU MIT rather than MIT | [Brand history](../product/brand-history-license-governance.md), [LICENSE](../../LICENSE) |

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
| AC-044 | OIDC provider refresh token reuse is detected under a rotation-capable IdP | Affected grant/session family blocked and incident recorded; IdP capabilities are verified; no false claim of guaranteed theft detection | ADR 0025, proposed token contract |
| AC-045 | Concurrent tab session renewals or lost response | Atomic state cannot resurrect stale token generation; legitimate session handling is defined and testable | Proposed token contract |
| AC-046 | Owner requests budget increase through normal renewed session without recent verified strong step-up | Critical action denied pending strong reauthentication; UI continuity alone cannot approve spending | ADRs 0002, 0025, 0027 |
| AC-047 | User revokes an individual device session or all web sessions | Server-side revocation effective for selected sessions, audited, without indiscriminately cancelling unrelated preapproved worker attempts | ADR 0027 |
| AC-048 | Product scope is assessed for web v1 | No implicit commitment to mobile app or Jarvis-like voice assistant; both explicitly remain optional future concepts | [Product scope](scope.md), ADR 0027 |

These are *specified future tests, not executed*. The proposed implementation mechanisms are not owner-approved by documenting these scenarios.

## Owner-controlled project invitations — future acceptance cases

| Test ID | Scenario | Expected result | Requirement |
| --- | --- | --- | --- |
| AC-049 | Editor or Viewer requests Editor or Viewer invite | Rejected: **only Owner** may invite; role checks server-side | PR-021, PR-023 |
| AC-050 | Owner invites Editor to project A, token is opened without sign-in | Token alone cannot confer access; valid verified OIDC identity and matching recipient constraints required | PR-023 |
| AC-051 | One-use invitation is redeemed concurrently twice in SQLite | At most one atomic successful redemption and membership grant | PR-012, PR-023 |
| AC-052 | Invitation is redeemed at/after 24 hours from issuance, revoked/tampered, or addressed to another authenticated account | No membership grant and no cross-project data leakage | PR-013, PR-023 |
| AC-053 | Owner attempts transfer without recent verified strong step-up or target OIDC authentication/explicit acceptance; also simulate failed final commit | Denied/incomplete until both approve; original Owner remains authorized until atomic successful handover; no ownerless project | PR-021, PR-023 |
| AC-054 | New Editor after successful invite attempts to raise budget, expand Git/provider scope or activate unapproved DAG | Denied: membership never overrides prior Owner approvals or frozen execution plan | PR-008, PR-019, PR-023 |

All scenarios are **specified only**. No end-to-end implementation or validation has been performed.

## Multiple Owners — specified future tests

| Test ID | Scenario | Expected result | Requirements |
| --- | --- | --- | --- |
| AC-055 | Add a second Owner with authenticated current Owner step-up and prospective Owner OIDC login/explicit acceptance | Both independently verified identities hold scoped Owner membership; no inherited session or role confusion | PR-021, PR-023 |
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

## Evidence rules

Each implementation claim needs source commit, repeatable test command, environment/version, result and known limitations. A proposed acceptance test is not a completed test. Automated checks should run in CI before merging behavior changes; security-critical behavior must include negative tests.

## Pending decisions

Performance thresholds, service reliability targets, supported worker providers, concrete authentication tests, timeout policy, UX/approval presentation and which tests gate initial release.

See [testing strategy](../development/testing.md), [project status](../project/status.md), and [decision register](../decisions/README.md).
