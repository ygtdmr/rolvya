# ADR 0039 — AWS ECS on Fargate replaces Hetzner Cloud in the initial worker adapter priorities

Status: **Approved**  
Date: 2026-10-24  
Revises: [ADR 0018 — Initial Remote Worker Adapter Priorities](0018-initial-worker-adapters.md), **provider number 2 only**

## Owner decision

The Owner explicitly requested replacing **Hetzner Cloud** with the **best-fitting AWS service** for Rolvya. The selected AWS integration is **Amazon ECS using the AWS Fargate launch type**, specifically **on-demand ECS tasks via RunTask**, not an always-on ECS Service or a second independently controlling job scheduler.

The active approved initial adapter priorities are now:

1. **Google Cloud Run Jobs** — managed serverless batch-container jobs.
2. **Amazon ECS on AWS Fargate** — directly launched, short-lived, isolated remote coding worker tasks.
3. **User-enrolled remote VPS** — independently verified user-provided remote hosts, never the user's personal computer.

This supersedes **only** the old ADR 0018 second entry (Hetzner). Multi-provider architecture [ADR 0011](0011-multi-provider-worker-infrastructure.md), host categories [ADR 0017](0017-worker-host-categories.md), remote-only policy, immutable preplanned DAG, [worker HTTPS](0006-worker-communication.md), Secret Broker [ADR 0037](0037-central-secret-broker-task-scoped-access.md), multiple eligible CLI/API connections [ADR 0038](0038-multi-account-isolated-cli-and-api-access.md), worker token/lease timers and **hard budget admission** are unchanged.

## Why ECS with Fargate for Rolvya

- **Rolvya retains its own scheduler, task DAG, retry and hard cost reservations.** ECS `RunTask` launches one explicitly admitted execution without delegating its approval, budget or retry logic to AWS Batch queues.
- **Ephemeral, isolated container tasks** avoid paying for provisioned idle VM workers by default; CPU/memory and allowed network/IAM policy can be specified for the particular task. The AWS Fargate Linux billing model has a **one-minute minimum**, with duration starting from image download and continuing until the task stops; additional resources, storage, registry, networking and monitoring can add charges.
- **RunTask supports caller-supplied clientToken idempotency**, useful for recovering from lost responses; it is not permanent deduplication or proof that the process stopped. Persist the operation key, inspect the actual task, fence the Rolvya attempt and reconcile charges before another launch.
- **Fargate provides a stronger host-level separation** than assuming a rootless container on an arbitrary shared VPS is a universal hostile-code sandbox. Nevertheless task IAM, egress, secret handling, filesystem boundaries and real deployment tests remain mandatory.

## Alternatives and limitations

- **AWS Batch with Fargate:** useful for large independent queued batch workloads, but adds AWS-owned scheduling/queue/retry behavior to Rolvya's already defined authoritative scheduler. It is not the initial adapter choice.
- **EC2 instances / ECS on EC2, Spot or ECS Managed Instances:** may later suit sustained CPU-intensive builds, higher-utilization pools, GPU/special kernels, privileged operations and custom images. More host and interruption/budget management is required. **No EC2 fallback is silently approved** by this selection.
- **AWS Lambda:** not selected as the coding-worker process host; long-running CLI executions, flexible resources and interactive build/test processes better match container tasks.
- **Fargate constraints:** no privileged containers, host Docker socket or conventional Docker-in-Docker; Docker image builds needing such capabilities need an approved alternative. Fargate filesystem is **ephemeral (20 GiB by default, configurable up to 200 GiB on ECS)**. Persistent Claude/Codex account grants must reside in trusted encrypted Secret Broker storage, not inside terminated Fargate tasks; isolate a runtime credential projection per connection/attempt only when supported/authorized.
- **CLI provider entitlement remains OQ-008:** ECS capability does not grant provider permission for unattended subscription-backed CLI use. Block unsupported modes; API-key mode remains distinct and must be explicitly funded.

## Mandatory first implementation validation

- Use fake-model jobs and synthetic code first; **no paid provider execution without explicit budget approval**.
- Prove `RunTask`/clientToken idempotency, `DescribeTasks` reconciliation under eventual consistency, stop/cleanup, role/identity separation, task-level secrets and account-specific CLI runtime isolation.
- Verify Fargate regional quotas, task startup/cancellation latency, CPU/RAM, appropriate networking and AWS-specific hidden costs (e.g. image pulls, NAT/data transfer/logging, idle services); show optimistic/expected/pessimistic costs separately from a *hard enforceable reservation*.
- Verify workload compatibility with lack of privileged Docker; don't silently change task model/mode/provider, add graph nodes or bypass worker lease/budget approval.

## Approval boundary

**Approved:** swap the second initial priority to Amazon ECS **on AWS Fargate**.  
**Not yet approved:** AWS region, exact task CPU/RAM/image, AWS account/IAM setup, actual prices, provider benchmark spending, production safety certification, initial public release requirement for one vs multiple live adapters, whether to use Fargate Spot, and any automatic EC2 fallback.

The previously drafted [staged rollout proposal](../architecture/initial-remote-worker-rollout-proposal.md) remains **Proposed** for the exact development/release sequencing; this ADR does not turn that proposal into Owner-approved release scope. No AWS services have been deployed or tested through this ADR.

## Official evidence (reviewed 2026-10-24)

- [AWS Fargate pricing](https://aws.amazon.com/fargate/pricing/) — task vCPU/memory and one-minute Linux minimum.
- [Amazon ECS RunTask idempotency](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ECS_Idempotency.html) — task launch client token and time-limited safe retry.
- [Amazon ECS RunTask API](https://docs.aws.amazon.com/AmazonECS/latest/APIReference/API_RunTask.html) — eventual consistency and task launch semantics.
- [AWS Fargate security considerations](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-security-considerations.html) — privileged and host-runtime restrictions.
- [Fargate task ephemeral storage](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-task-storage.html) — default and maximum temporary storage.
- [AWS Batch versus ECS](https://docs.aws.amazon.com/en_en/batch/latest/userguide/bestpractice1.html) — batch queues and scheduler tradeoffs.
- [AWS Batch when to choose Fargate vs EC2](https://docs.aws.amazon.com/batch/latest/userguide/when-to-use-fargate.html) — resource and feature limitations.

Related: [ADR 0018](0018-initial-worker-adapters.md), [provider-neutral adapter contract](../architecture/multi-provider-adapter-contract.md), [rollout proposal](../architecture/initial-remote-worker-rollout-proposal.md), [provider benchmark plan](../operations/worker-provider-evaluation.md), [open questions](../project/open-questions.md).
