---
title: Backup and Disaster Recovery
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [backup, recovery, database, resilience]
---

# Backup and disaster recovery

## Scope and guarantees

Rolvya uses **SQLite as its approved single authoritative database** ([ADR 0005](../decisions/0005-authoritative-database.md)), accessed through the **Rust-only backend** ([ADR 0004](../decisions/0004-control-plane-language.md)). Single-instance design simplifies coordination but creates a failure boundary; backups and verified restore procedures are essential. SQLite-specific operational conditions are in [SQLite operations](sqlite-operations.md). This document is a **proposed policy**, not evidence that snapshots, retention, storage providers or restore tests exist.

Protect at minimum: the fixed versioned plan graph, task/attempt states, verified artifacts and hashes, approved budget ceilings, approvals, cost reservations, settled entries, remote repository refs/operation IDs, immutable decision history, required configuration and encryption key references.

## Recovery objectives

Define and approve both:

- **RPO (recovery point objective):** maximum permissible loss of durable state; human budget approvals and charge ledger events likely demand especially strong guarantees.
- **RTO (recovery time objective):** maximum duration to restore service, including integration/billing reconciliation and worker fencing.

No RPO/RTO numbers have yet been approved. Do not imply a backup exists merely because a scheduled command returned successfully.

## Proposed backup architecture

| Source | Backup unit | Key requirements |
| --- | --- | --- |
| Authoritative database | Transaction-consistent snapshot plus suitable log/delta capture | Restore integrity; plan graph and ledger together |
| Object/artifact storage | Versioned objects/manifests and checksum inventory | Preserve verified outputs; prevent orphan references |
| Configuration | Versioned nonsecret configuration and infra definitions | Credentials only as secret-manager references |
| Secret manager | Provider-approved recovery/rotation procedure | Never store plaintext in repository or unencrypted backups |
| Remote Git | Durable provider refs/commit IDs and repository identity | Reconcile against upstream; do not assume local history exists |
| Audit log | Durable protected event history | Reconstruct approvals and ambiguous external operations |

Encrypt in transit and at rest as appropriate; constrain backup access and maintain independent storage/failure domains where feasible. Define retention, legal deletion, cost and rotation schedules via ADR.

## Consistent restore sequence (proposed)

1. **Incident assessment:** authenticate operator; suspend new billable dispatch, freeze or fence worker leases, preserve evidence.
2. **Find valid restore point:** verify backup provenance, cryptographic/checksum integrity, compatible schema and manifest continuity.
3. **Restore isolated environment:** use a clean remote host, least-privilege credentials, compatible application/schema and object store.
4. **Rebuild relationships:** validate DAG acyclicity/immutability, attempt fence epochs, required artifacts, approvals, reservations and cost entries.
5. **Reconcile external side effects:** remote Git provider refs, successful but unacknowledged writes, cloud worker lifetimes, API charges and pending reservations.
6. **Revoke stale authority:** rotate compromised credentials, invalidate old worker leases, fence stale attempts; never accept old success callbacks.
7. **Validate readiness:** run negative budget admission checks, dependency constraints, artifact reads and audit reconstruction.
8. **Resume cautiously:** only after authorized operator acceptance; a project paused for budget must remain paused without explicit approval.

Restoring a stale budget ledger cannot justify replaying work that may already have been billed. Treat missing provider usage as uncertain exposure and hold risky new admission.

## Historical storage migration recovery (Approved policy; Proposed mechanics)

[ADR 0030](../decisions/0030-project-activity-audit-log.md) approves **staged, resumable historical archive/checkpoint migration** after separate Owner authorization, without automatic source deletion. The [proposed migration state machine](../architecture/storage-migration-state-machine.md) describes one candidate journal and verification design.

During disaster recovery, restore the **migration manifest and durable per-object verification evidence** alongside SQLite approvals and budget reservations. Fence old migration leases; reconcile provider writes and delayed/unknown bills before retrying. A verified object can be reused only while its source version and independent destination evidence remain valid. Never treat mere upload success as verified completion.

A storage target cutover affects new writes only. Interrupted or budget-paused migration retains the original provider data and protected trust anchors. **ADR 0030 additionally approves finite automatic retries with exponential backoff for classified transient migration transfer failures.** Restore durable retry counts, wait-until timestamps and provider operation identity so a controller restart does not reset attempts or cause blind chargeable replays. Reconcile ambiguous provider results and known/uncertain charges before an eligible retry; use no automatic retry for verified corruption, forbidden provider/permissions, legal holds or insufficient approved headroom. Do not invent a successful migration when the destination cannot be verified. Apply legal holds, retention and independent cleanup approval even after full transfer. **Approved retry default: 4 total storage-transfer attempts (initial plus up to 3 automatic retries).** The counter must survive recovery; a fifth automatic attempt is forbidden. Exact backoff timing/jitter, backup schedule, cost enforcement and provider checkpoint methods remain unselected and not implemented.

## Failure modes and mitigations

- **Corrupt or incomplete snapshot:** reject and use prior verified backup or repair with evidence.
- **Database failure during financial reservation:** replay durable transaction/event log and reconcile external charge before admitting work.
- **Artifact missing:** mark affected validation state uncertain; do not present it as completed.
- **Key unavailable:** secrets recovery requires provider-approved recovery plan, not embedding credentials in backups.
- **Remote Git history changed:** compare provider commit/ref SHAs, stop silent branch overwrite.
- **Region/provider outage:** alternative region/provider is an open deployment choice and may incur transfer charges requiring budget authority.

## Restore testing

Run test restores to isolated remote infrastructure with fake-provider adapters and synthetic data. Exercise an interrupted reservation, orphan attempt, late cost invoice, unacknowledged successful Git push, missing artifact and compromised credentials. Record precise restore commands, versions, hashes, elapsed time, data loss estimate and remaining gaps.

Do **not** perform destructive production recovery as a documentation test.

## Operator checklist and evidence

Before declaring recovery successful, produce: incident ID, triggering symptoms, backups used and integrity evidence, pre/post database schema version, exact restored event sequence, Git/artifact reconciliation summary, worker lease cleanup proof, budget reconciliation, RPO/RTO observed and approving operator.

## Open decisions

Chosen database and snapshot method, backup cadence/retention, encryption/key manager, object-storage versioning, warm standby, approved RPO/RTO, restore test frequency, data deletion/legal compliance and secondary-region costs.

Related: [deployment](deployment.md), [runbooks](runbooks.md), [data model](../architecture/data-model.md), [failure recovery](../architecture/failure-recovery.md), [budget](../architecture/budget-costs.md), [secrets](secrets.md).
