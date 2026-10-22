# ADR 0020 — Provider-aware Worker Isolation

Status: **Approved**
Date: 2026-10-22

## Decision
Rolvya enforces uniform minimum security boundaries for executing untrusted coding workloads while allowing the concrete sandbox technology to vary by remote provider and host capability. Approved by owner on 2026-10-22.

- Google Cloud Run Jobs: use managed per-execution isolation plus Rolvya-scoped identities, filesystem/network/resource limits; validate effective provider security capabilities.
- Hetzner Cloud: evaluate gVisor as a preferred stronger sandbox candidate, subject to compatibility, kernel/runtime features and benchmarking; do not assume it is deployed or supported.
- Enrolled user remote VPS: inspect kernel/container/virtualization capabilities and fail closed for untrusted execution if adequate isolation is unavailable.
- Rootless Docker/Podman alone is not considered a universally sufficient hostile-code security boundary. Firecracker microVMs are a possible alternative only where required virtualization support is available.

## Required safeguards
Every worker attempt is isolated from other projects and attempts, with bounded CPU, memory, disk and runtime; restricted filesystem and network access; least-privilege identities and secrets; no unapproved host mounts, privileged containers or cross-tenant credential access. Provider-managed isolation claims must be verified, not presumed. Host limitations must be reflected in scheduler eligibility, not overridden to improve throughput.

Keep approved hard budget and fenced lease policies, remote-only execution, immutable DAG, and trusted remote Git publication. Implementation details and tests remain proposed; no sandbox implementation or deployment is asserted.

Related: [host categories](0017-worker-host-categories.md), [initial adapters](0018-initial-worker-adapters.md), [adapter contract](../architecture/multi-provider-adapter-contract.md).
