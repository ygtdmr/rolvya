# Storage Provider Choice and Google Drive Eligibility

Status: **Proposed implementation; owner-approved product direction**
Date: 2026-10-22

## Product decision
Rolvya aims to offer users a **broad, extensible choice of backup/storage providers**, not just one hard-coded vendor. Users can select supported provider accounts/destinations for backups, and may configure primary and secondary destinations after reliability and compliance checks. Provider integrations can ship incrementally; a universal adapter architecture does **not** imply every provider works in the first release.

Keep separate interfaces for:
1. **Compute/worker providers** (remote execution, lifecycle, cost reconciliation).
2. **Backup/storage providers** (write, read, list/version, verify, retention, restore, delete subject to owner policy).

Potential storage candidates include S3-compatible stores (AWS S3, Cloudflare R2, Backblaze B2 and compatible hosts), Google Cloud Storage, Azure Blob, and approved remote storage/server targets. Each implementation must verify capabilities and limits. Avoid claiming broad compatibility without actual adapter tests.

## Google Drive
Owner explicitly requested that Google Drive be offered **as an optional backup destination**, not merely project file import/export. This is a requested capability, **not yet approved for deployment**.

As checked 2026-10-22, [Google Drive API Terms of Service](https://developers.google.com/workspace/drive/api/terms) state that backing up user or app content from a developer app/project to Drive is prohibited **without Google's express prior written consent**. The [Google Workspace developer policy](https://developers.google.com/workspace/workspace-api-user-data-developer-policy) similarly describes this use case as prohibited. **OAuth authorization by a user alone does not waive the separate restriction.**

Therefore:
- Track a Drive backup adapter as **blocked pending verified eligibility/express written consent or an applicable formal allowance**, with documented legal/policy review; do not advertise or enable backup support before eligibility is established.
- Google Drive file selection/import/export may be designed separately within the appropriate OAuth scopes and approved use cases, but must not be used as a disguised backup workaround.
- Do not assume that Drive appData, manual selection, user-owned accounts, or encryption bypasses the policy.
- Recheck current official policy at launch and record evidence.

## Cross-provider backup contract
Adapter capabilities: authorize/check permissions, put immutable encrypted snapshot and manifest, confirm checksum/integrity, list recovery points, retrieve, enforce retention, revoke/disconnect, report storage and transfer costs and quota/health. Implement async and durable retries with idempotency; avoid holding SQLite transactions during uploads. Protect credentials and encryption keys outside worker prompts. Backup only coherent WAL-aware SQLite snapshots; preserve the approved 15-minute/daily cadence and 7-/30-day retention unless changed by the owner.

Provider failure must not silently mark a backup successful. Support explicit fallback destination only if pre-authorized and within hard spending caps. Recovery verifies snapshot, restores in isolation or under controlled outage, reconciles worker leases, provider jobs and financial exposure before new billable work.

## Acceptance conditions
- User can pick among **actually implemented, verified and eligible** backup providers, with clear costs, quotas, health and primary/secondary destination settings.
- No assumption that all candidate providers are available in v1.
- 15-minute and daily backups fulfill integrity and restore tests across supported adapters.
- OAuth credentials, backup contents, billing and recovery state remain project-scoped.
- Google Drive backup option stays unavailable until policy eligibility is documented.

References: [Backup and recovery ADR 0015](../decisions/0015-backup-recovery.md), [Multi-provider ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md).
