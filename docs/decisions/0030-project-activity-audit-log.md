---
title: "ADR 0030: Project Activity Log and Tamper-Evident Audit"
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [activity, audit, security, provenance, events, observability]
---

# ADR 0030 — Project activity log and protected audit history

- **Status:** Approved
- **Date:** 2026-10-22
- **Decision:** Rolvya maintains a project-scoped **Activity Log** showing material human, agent, worker, financial and security events. Ordinary users cannot edit or delete existing activity records. Sensitive data must not be revealed in event details.

## Motivation

Multiple authorized Owners, Editors, Viewers and independently authorized machine principals interact with one fixed DAG, bounded budget ledger, remote Git and cloud providers. Project participants need a trustworthy timeline of who did what, when, and what occurred, including failures, while preserving private credentials and tenant isolation.

## Approved requirements

1. **Project-scoped activity history:** Display key project events such as budget ceiling changes and budget pauses, membership/invitation/Owner changes, agent plan activation and task dispatch, worker task completion/failure/retry, approvals, critical system/security alerts and remotely published Git results.
2. **Clear provenance:** Record timestamp, project ID, actor class and verified actor reference (human Owner/Editor, main agent, scheduler, worker, provider integration or system), event type, involved task/attempt IDs where relevant, outcome and useful bounded context. Differentiate user requests from actual server-verified effects; never fabricate actor identity.
3. **Append-only at application level:** Ordinary users, including Owners, must not be able to rewrite or silently delete historical event records. Corrections use a later compensating event, preserving the original fact. Rust enforces authorization and atomic write rules through authoritative SQLite.
4. **Integrity and recoverability:** Protect against accidental loss or unauthorized alteration; auditing should be durable, backed up consistently with project state, access controlled and capable of detecting unexpected modifications. This is a design requirement, **not** a claim that SQLite alone is immutable against a root/server/database administrator. **Cryptographic tamper-evidence is Approved as mandatory.** Independent external anchoring, algorithms and key management remain implementation choices; a local administrator with full access may still be able to forge or suppress unanchored histories.
5. **Secret hygiene:** Redact or exclude raw credentials, tokens, passkeys, invitation secrets, private keys, user prompts containing secrets, private code snippets, sensitive worker stdout and other confidential data. Store safe structured summaries or pointers to separately authorized artifacts. Apply retention minimization to personal identifiers.
6. **Role-aware viewing:** Owner and Editor may inspect activity permitted by project membership; Viewer can see an authorized read-only subset. Sensitive security events and actor metadata should be displayed according to independent least-privilege access rules, not made globally visible merely because a user can view the project. Cross-project leakage is prohibited.
7. **Separate billing authority:** A timeline entry or notification is **not** proof that additional spend was approved. Approved budget/lease transactions and human approval records remain authoritative; Activity Log mirrors meaningful events without granting spend or roles.
8. **Useful UX and economy:** Provide chronological timeline and practical filtering (actor, event category, task, time) without requiring a new external observability service on the initial small Rust/SQLite controller. Retention, pagination and archival must balance resource constraints and privacy.
9. **Resilient event ingestion:** Critical events such as approval commits, membership changes and state transitions must have a crash-safe route into activity history (e.g., same SQLite transaction or durable outbox), with deduplication for retries. External exporter failure must not invent event success or silently override security rules.

## Approved active-history retention and archival (2026-10-22)

- Keep **90 days of project Activity Log events readily accessible in the normal project panel**. This is an active-access window, **not automatic permanent deletion** after 90 days.
- Once an event ages beyond this active window, it **may be moved to a secure compressed archive** to minimize resource use on the small Rust/SQLite controller. Archival must preserve integrity, authorization, chronological provenance, search/retrieval via an authorized workflow, and safe recovery.
- **Critical budget approval, financial accounting, ownership/security and incident records may require longer retention** than routine activity events. The **approved total durations are 1 year for routine activity and at least 3 years for critical finance/security events**, subject to applicable law. Archive location, retrieval service levels and detailed privacy/legal deletion workflow remain engineering decisions.
- The transition from active storage to archive must be resumable, audited and loss-resistant, with no ordinary user erase/edit capability. Avoid claiming SQLite itself is tamper-proof.

## Approved cryptographic integrity verification (2026-10-22)

- Material project audit records **must be cryptographically verifiable** so that post-hoc modification or removal can be detected when checked against a trusted integrity reference. Design for authenticity of the sequence, not just a checksum of each isolated event.
- Verification must cover **stored history and archived history**, including archive/export/restore transitions, sequence gaps and unauthorized record rewrites. Treat failed verification as an actionable security incident with clear status rather than silently rendering altered events as authentic.
- Define a trustworthy verification root or protected signing/key mechanism independent enough from ordinary mutable event storage to make tampering detectable. A plain hash chain whose head is freely rewritable by the same attacker is not sufficient protection against full-chain replacement.
- The choice of algorithm, signing-key custody, trusted checkpoint or remote anchoring technique, rotation procedure, verifier frequency and independent recovery is **open for engineering review**, not pre-approved or implemented. It is **not guaranteed** to detect all attacks by an administrator who also compromises the integrity trust root.
- Lawful, authorized retention expiration/anonymization must be accounted for using a verifiable lifecycle strategy; it should not be misreported as malicious deletion.

## Approved independent, provider-neutral integrity checkpoint storage (2026-10-22)

- Store trusted audit-integrity **checkpoints independently from the authoritative SQLite database**. This separation is a required architectural boundary, not a claim of protection if both systems and signing credentials are compromised together.
- Use a **pluggable storage adapter/interface** rather than locking Rolvya to S3. **S3-compatible object storage is one supported adapter option, not the only option.** Other qualified object-storage services, self-hosted remote storage or equivalent providers can be integrated where they offer the necessary durability, access controls, integrity retrieval and operational cost visibility.
- Where supported, use versioning, object retention or write-once controls to harden checkpoint integrity. Do not assert that all providers support equivalent immutability; check capabilities and refuse to mark an unprotected checkpoint as independently trusted.
- Keep access credentials, signing keys and checkpoint write controls distinct from ordinary SQLite write authority; protect against unauthorized overwrite and rollback, and ensure archive/restore verification can consult the protected checkpoint source.
- Exact initial non-S3 adapter choices, backup quorum, checkpoint cadence and cryptographic algorithms remain engineering decisions. No vendor-only requirement, local-computer sync or personally hosted worker is introduced.

## Approved storage provider governance (2026-10-22)

- **The Rolvya deployment/system administrator controls which storage backends are configured, credentialed, enabled and permitted.** A configured adapter is not automatically approved for every project; the administrator's allowlist and capability/security policy are authoritative.
- **Project Owners can choose only from storage targets explicitly allowed for their project by that administrator.** Editors, Viewers, AI main agents and remote workers cannot register new storage providers or send checkpoint/archive data to unapproved endpoints.
- Rust enforces allowlisted project/provider binding, destination and region constraints, least-privilege secret references, integrity protections and applicable storage/transfer budget before any write or migration. Changes are audited, and credential material never enters AI context or project user-visible logs.
- Reconfiguration or migration cannot silently invalidate protected checkpoints. Existing data remains verifiable and recoverable under approved lifecycle and transfer rules. No automatic external storage access is assumed if an administrator has configured no eligible target.
- **Multiple providers remain supported** by the extensible storage interface; the administrator may authorize S3-compatible, other cloud or eligible self-hosted remote adapters. Exact UI, credential vault, target health probes and migration mechanics remain engineering questions.

## Approved explicit storage migration and safe destination changes (2026-10-22)

- **Changing a project's configured storage target is not authorization to copy or relocate existing data.** Switching an Owner-selected, administrator-allowlisted provider applies only to newly eligible writes after the replacement destination is verified. Existing archives, integrity checkpoints and other pre-existing stored data **remain where they are** until a separately approved migration is initiated.
- **Validate the destination before cutover:** confirm the adapter and endpoint are still administrator-permitted for that project; verify credentials, required durability/retention capabilities, encryption/access policy, integrity-checkpoint controls, health and applicable spending constraints. Failed verification blocks cutover and does not silently fall back to an unapproved target.
- **Old-data migration is a distinct Owner-initiated operation, with explicit approval of its scope and financial exposure** under the existing hard budget gates. No background agent, worker, switch UI event or administrative allowlist update may infer consent to billable data copying. Preserve exact source/destination identities, audit state and expected transfer costs before beginning.
- Migration must protect data integrity and continuity: stage/copy and independently verify migrated archives and trusted checkpoints before a target is treated as authoritative; handle partial transfers, retries and restore, and do not accidentally replace trusted checkpoints with unverified ones. **Do not silently delete old copies** as a side effect of selecting a target or of a failed migration.
- Where switching the configured target makes old records temporarily unavailable, show that honestly and retain safe access/recovery instructions; do not claim data were migrated or fully accessible. Whether/when old redundant copies can be cleaned up is a separate governed retention/deletion decision.
- Exact verification probes, state-machine enum/transaction implementation, transfer limits, cross-provider copy method and final source cleanup workflow remain unselected engineering details; **resumability and safe recovery behavior are approved above**. These requirements are approved architecture, **not implemented code**.

## Approved resumable historical migration and recovery (2026-10-22)

- After a separately authorized historical migration begins, execute transfer in **bounded, staged units** so an interruption does not invalidate already independently verified work. Persist durable progress rather than relying on a single uninterrupted provider job.
- If **1,000 historical objects** were approved for migration and **800 were independently verified** before a network failure, a safe restart **reuses still-valid verification evidence for those 800** and resumes or re-verifies the remaining 200 rather than blindly re-copying all 1,000. A copied or acknowledged item is not independently verified. This numerical example illustrates approved resumability, not an approved chunk size or service guarantee.
- **Retain all original source objects and protected audit-integrity anchors** throughout preparation, partial success, pause, cancellation, failure, recovery and verified completion. No copy, retry, resume or completion path implies source deletion, and no existing active data or verified destination copy may be silently overwritten.
- Before resuming after timeout, controller restart, lease expiry or uncertain provider acknowledgement, reconcile recorded object versions, already verified destination evidence, any in-flight external writes, stale actors and **known/uncertain provider costs**. Retry only unfinished or unverified items that still fall within the separately approved source/target scope and eligible financial envelope. A manifest/source change or forbidden destination may not be silently substituted.
- **Honor hard project-budget admission and bounded in-flight exposure on every billable unit/retry.** When remaining approved headroom is insufficient or enforceable limits are lost, pause new billable transfer activity (and stop/fence unbounded in-flight work as appropriate) while preserving successfully verified items, source data and journal. An increase to the budget ceiling requires **new explicit Owner approval**, never an automatic buffer or retry-driven overrun.
- Migration reaches **verified completion only after independent full-manifest** completeness, retrievability, expected version/content integrity and protected checkpoint/trust continuity checks, followed by durable recording of the outcome. Partial failure, unverified entries or broken integrity anchors must be shown as incomplete/paused/blocked, not as completed.
- Preserve audit evidence for approvals, attempts, costs, failures, pauses, resumption and final verification without secrets. **No automatic old-source cleanup** follows successful migration; the separately approved cleanup and mandatory retention/legal-hold conditions below continue to govern.
- Exact state names, SQLite schema, manifest serialization, multipart/object sizing, retry count/backoff, provider checkpoint APIs, cryptographic verification mechanics and UI are **Proposed implementation details**, not approved numeric parameters. See the [proposed storage migration state machine](../architecture/storage-migration-state-machine.md). **No migration code or tests are claimed implemented.**

## Approved bounded automatic retry for historical migration (2026-10-22)

- **Bounded automatic retries with exponential backoff** are allowed for **transient** historical storage-transfer failures only: temporary connection loss, timeout, provider throttling/rate limits or a confirmed transient provider-side outage. Retries must be finite and use a bounded, increasing delay; do not hammer an unavailable provider. Observe provider-declared wait/rate-limit constraints where applicable.
- **No automatic retry** for failed destination cryptographic/content-integrity checks, changed or unverifiable source manifest, invalid/revoked authorization, administrator-disallowed provider/region, retention/legal hold, security incident, or insufficient/unbounded approved spending exposure. A temporarily unavailable verification *read* may be retried within the same limits, but a **verified mismatch is not treated as a transient read failure**.
- A retry is permitted only inside the **unchanged separately Owner-approved migration scope**, while security/provider/retention policy still permits the operation and fresh **hard-budget admission** covers any new billable exposure. Do not silently increase the project ceiling, make billable replays on unknown provider outcomes, move data to another target, or delete source objects.
- **Persist retry and waiting state durably**, reconcile ambiguous remote writes and charges before reissue, fence stale attempts, preserve individually verified progress and original source data. Duplicate callbacks or controller restart must not reset retry limits or schedule redundant paid transfers.
- **Approved default for each logical storage-transfer operation: 4 total execution attempts, counting the initial attempt plus at most 3 automatic retries.** A fourth *retry* (fifth total attempt) is forbidden. Only classified transient failures are eligible; security, integrity, authorization and budget rules can stop attempts sooner. Preserve the attempt counter across restarts, retries and recovery; never reset it silently to bypass the ceiling. The exact per-object/part operation boundary remains an engineering definition to be validated against provider APIs and billing semantics.
- **After 4 total attempts have been exhausted**, pause/block further automatic work and expose the cause, verified progress and cost uncertainty to authorized users for recovery or a separately governed policy change where needed. No infinite retry loop; legitimate resumption revalidates authorization, manifest, provider and budget, and does not silently grant another attempt.
- This **storage-transfer default of four total attempts** is separate from the approved default of **three total AI coding-worker task attempts** in [ADR 0019](0019-centralized-worker-retries.md). Neither policy changes the other or creates additional billable authority.
- **Approved numeric retry ceiling:** 4 total storage-transfer attempts (initial + up to 3 automatic retries). **Still open:** timeout/backoff base and cap (the previously mentioned 5-second starting delay has not been separately approved), jitter implementation, `Retry-After` handling details, operation granularity and per-provider throttling/concurrency controls. These are **not implemented**; the technical candidate design is in [storage migration state machine](../architecture/storage-migration-state-machine.md).

## Approved post-migration source retention and Owner-controlled cleanup (2026-10-22)

- **Never automatically delete the old provider's source data**, even after an explicitly approved migration reports success and the destination is independently verified. The source archives, historical records and protected integrity checkpoints remain intact; destination selection and migration authorization do not grant cleanup permission.
- Before any migration-related cleanup, independently verify destination **completeness, retrievability, cryptographic integrity and continuity of independently trusted checkpoints**. A transfer-complete message or checksum from the copying worker alone does not establish sufficient verification.
- **Source cleanup is a third, distinct and explicit project-scoped operation** requiring **strongly authenticated project Owner approval**, separate from (1) choosing a storage destination and (2) approving migration. Before that approval, show the exact source provider, object scope, verification evidence, expected storage/transfer/cleanup cost exposure, and potentially irreversible consequences. Editors, Viewers, agents, workers and background callbacks cannot grant or infer cleanup approval.
- **Owner approval cannot bypass** administrator/provider allowlists or capabilities, mandatory retention, legal holds, security constraints, or applicable privacy and lawful-erasure duties. Routine activity has a **1-year total retention** policy; critical financial/security history has **at least 3 years** subject to law. If the destination is unverified or an applicable hold/retention requirement blocks deletion, deny or defer cleanup without modifying the source. Separately governed retention-expiry or legally required erasure operations are not implied migration cleanup.
- Scope every authorized deletion to the approved objects and execute with least-privilege provider credentials, bounded restart-safe progress, and auditable outcomes. Record the verified Owner actor, approval, time, scoped object references, integrity evidence, attempts, failures and confirmed results in the protected Activity Log without exposing secrets. A partial or ambiguous provider response must never be reported as confirmed deletion.
- Before deleting any historical verification reference, preserve a separately protected surviving reference adequate to verify remaining historical records; **never erase the last trustworthy evidence** of authenticity. Apply consistent backup/export lifecycle and provider-specific irreversibility safeguards.
- The exact step-up mechanism, confirmation UX, deletion APIs, batch limits, idempotency handling, retention/backup purge procedure and attestation details remain **open engineering choices**. **No migration, deletion or verification implementation is claimed.**

## Approved total retention windows (2026-10-22)

- **Routine project activity events:** retain for **1 year total from event creation**, including the first **90 days readily accessible in the project panel** and the remainder in a protected compressed archive when archival is enabled.
- **Critical financial, budget-approval, ownership, security and incident events:** retain for **at least 3 years total**, subject to applicable law, legitimate retention need and privacy safeguards. Where relevant law requires shorter deletion or longer preservation, legal obligations take precedence.
- **Expiration does not authorize unchecked deletion:** apply documented, project-scoped, permissioned lifecycle controls and auditable deletion/anonymization after applicable retention expires; preserve legitimate legal holds where appropriate. Backups and exported archives need consistent lifecycle handling, not hidden perpetual copies.
- **Minimize confidential/personal data** regardless of retention category. A record's category must be assigned by server policy, not by a worker or untrusted content. No ordinary role may edit existing events or bypass retention workflows.

## Boundaries and open engineering choices

- Exact list and severity of event types, data minimization schema, timestamps and ordering strategy, user-facing search filters, notification linking and pagination are unselected implementation details.
- Exact **archive technology and lawful deletion/anonymization procedures** (the **90-day active panel, total 1-year routine retention and minimum 3-year critical retention** are Approved), per-role view filtering for particular event categories, cryptographic implementation and trusted anchoring design, compliance/legal deletion exceptions and disaster recovery verification are **not** approved numeric or operational choices. Legal data-erasure duties can require a separately controlled retention/deletion workflow; the application-level no-edit/no-delete rule does not supersede the law.
- These approvals **do not** mean an Activity Log, event pipeline or UI already exists; this ADR specifies future behavior.

## Future tests (specified; not executed)

- Owner changes the ceiling with valid step-up: timeline contains actor, old/new amount, time and result, but no secrets. Repeated callback cannot duplicate the effective change.
- Editor lacks permission to invite a member; show denied security event where appropriate without leaking user data.
- Worker fails, retries then succeeds: task/attempt chain and reason are accurately shown with no fabricated completion.
- Owner, Editor and Viewer attempt to overwrite or delete an audit event: server rejects all ordinary mutations; append correction preserves provenance.
- Project A user attempts to access project B events: deny.
- Worker stdout contains a token: log output has safe redaction, and sensitive text is not leaked through activity filters or notifications.
- Crash between budget commit and Activity Log delivery: recovery reconstructs or delivers a single accurate durable event.
- Authorized Viewer sees only permitted events and does not acquire secret access from viewing event details.
- Controller restarts/backup restores preserve auditable sequence and can flag integrity anomalies.

## Rationale and rejected alternatives

- **Rejected:** editable activity rows as a regular project note feed; users could rewrite failures or approvals.
- **Rejected:** dumping all model prompts and worker logs into one public event stream; confidentiality and cost risks.
- **Chosen:** scoped structured activity records, app-level append-only behavior, secret redaction and durable provenance, with evidence-backed tamper detection planned.

## Related

- [Audit and observability architecture](../architecture/observability.md)
- [Human authorization](../architecture/identity-authorization.md)
- [Budget approval ADR 0002](0002-hard-budget-approval.md)
- [Owner governance ADR 0029](0029-owner-controlled-project-invitations.md)
