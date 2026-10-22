# ADR 0016 — S3-compatible backup adapter first

Status: **Approved**
Date: 2026-10-22

## Decision

Rolvya will implement an **S3-compatible object-storage backup adapter first**, with AWS S3, Cloudflare R2 and Backblaze B2 among the candidate compatible destinations. Compatibility must be validated per provider; a shared S3 API does not mean all optional APIs, credentials, versioning, retention or cost semantics match.

This is an **implementation priority**, not exclusive vendor selection or a guarantee that every named provider ships in the first release. A provider-neutral storage adapter interface will admit additional implementations (e.g. Azure Blob, Google Cloud Storage and approved remote storage) in later stages. **Google Drive is out of scope**, per owner direction.

## Required behavior

- Create SQLite-consistent snapshots with WAL awareness; encrypt and checksum before remote upload. Keep a versioned manifest and verify round-trip restore.
- Support user-approved primary and optional secondary destinations, configurable credentials and project isolation, region restrictions, retry/idempotency, integrity checks, cost/accounting, retention and deletion.
- Follow approved backup policy: 15-minute snapshots, daily recovery points, frequent recovery points retained 7 days and daily ones retained 30 days.
- Observe hard budget approvals; external storage/transfer costs may not bypass approved ceiling. Failure of one provider must not silently mark a backup complete or automatically use a new paid destination without prior authorization.
- Keep cloud worker/compute adapters separate from storage adapters.

## Validation and open implementation work

Test AWS S3, R2 and B2 separately for upload/download, encryption, checksums, listing, multipart edge cases, retention, auth failures, quotas, costs and verified restore. No deployed integration or measured results are claimed by this ADR. Select Rust SDK/library and rollout order as implementation decisions.

Related: [backup and recovery](0015-backup-recovery.md), [backup storage provider contract](../architecture/backup-storage-provider-contract.md).
