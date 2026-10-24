# ADR 0020 — Provider-aware Worker Isolation

Status: **Approved**
Date: 2026-10-22

## Decision
Rolvya enforces uniform minimum security boundaries for executing untrusted coding workloads while allowing the concrete sandbox technology to vary by remote provider and host capability. Approved by owner on 2026-10-22.

- Google Cloud Run Jobs: use managed per-execution isolation plus Rolvya-scoped identities, filesystem/network/resource limits; validate effective provider security capabilities.
- **Amazon ECS on AWS Fargate (approved initial adapter after ADR 0039):** use task-level isolation, minimum-scope IAM, network egress controls, bounded CPU/RAM and task-specific secrets; validate real provider guarantees. No privileged containers or conventional Docker-in-Docker. A later optional EC2 adapter is a separate choice.
- Hetzner Cloud: previously discussed VM candidate, **not an active first-three adapter priority after ADR 0039**. If reconsidered later, validate provider/kernel/sandbox compatibility; do not assume gVisor is supported.
- Enrolled user remote VPS: inspect kernel/container/virtualization capabilities and fail closed for untrusted execution if adequate isolation is unavailable.
- Rootless Docker/Podman alone is not considered a universally sufficient hostile-code security boundary. Firecracker microVMs are a possible alternative only where required virtualization support is available.

## Required safeguards
Every worker attempt is isolated from other projects and attempts, with bounded CPU, memory, disk and runtime; restricted filesystem and network access; least-privilege identities and secrets; no unapproved host mounts, privileged containers or cross-tenant credential access. Provider-managed isolation claims must be verified, not presumed. Host limitations must be reflected in scheduler eligibility, not overridden to improve throughput.

Keep approved hard budget and fenced lease policies, remote-only execution, immutable DAG, and trusted remote Git publication. Implementation details and tests remain proposed; no sandbox implementation or deployment is asserted.

Related: [host categories](0017-worker-host-categories.md), [initial adapters](0018-initial-worker-adapters.md), [adapter contract](../architecture/multi-provider-adapter-contract.md).
