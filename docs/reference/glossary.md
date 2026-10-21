---
title: Rolvya Architecture Glossary
status: Proposed
category: reference
last-reviewed: 2026-10-21
tags: [glossary, terminology, architecture]
---

# Rolvya architecture glossary

## Status

This glossary standardizes terms across proposed architecture documents. It does not silently approve unresolved technologies or change the original user-approved architectural constraints. Canonical decisions are in the [decision register](../decisions/README.md).

| Term | Meaning in Rolvya | Important distinction |
| --- | --- | --- |
| **Main agent** | One planning/diagnosis agent role generating a complete plan before activation and diagnosing failures | A model role, not permission to bypass Rust policies |
| **Rust orchestrator** | Trusted backend logic governing plans, scheduling, budget, authorization and result validation | Backend implementation language is **Approved: Rust only** |
| **Task** | A planned unit with fixed identity, goal, dependencies and acceptance contract | Exists as a node before execution; not recreated for retries |
| **Task hierarchy** | Parent/child decomposition tree of a project's work | **Not** identical to execution prerequisites |
| **Dependency edge** | Directed relation A → B meaning B requires validated result of A | Explicit; neither automatic from hierarchy nor arbitrary parallel |
| **DAG** | Directed acyclic graph of prerequisite edges | All nodes and edges created and validated before activation |
| **Plan version** | Immutable snapshot/hash of task definitions and edges after activation | Separate from mutable runtime state; supersession policy Open |
| **Activation** | Transition from validated draft to executable immutable plan | Approval requirements and exact workflow to be selected |
| **Executable task** | Planned task intended to run on a worker | Whether group/parent tasks themselves execute is Open |
| **Group task** | Organizational parent grouping planned descendants | Aggregate success rule not automatically inferred |
| **Ready** | Dependencies validated; admission checks may still require budget, capacity, identity and nonconflicting refs | Ready is not proof of launched or funded work |
| **Admission** | Backend decision to start a billable attempt after rechecking eligibility | Budget and lease persisted atomically in SQLite |
| **Attempt** | One bounded worker execution instance of an existing task | New `attempt_id`; same `task_id` on retry |
| **Lease** | Time-bound current authority granted to one attempt/worker | Must be fenced; heartbeat timestamps alone insufficient |
| **Fencing epoch** | Monotonically increasing version disallowing stale attempt writes | Authenticated current epoch required for authoritative progress |
| **Outbox** | Durable SQLite intent written with an attempt and cost reservation before external dispatch | Handles crash replay; does **not** guarantee exactly-once external effects |
| **Worker** | Remote process/container executing a scoped task using permitted tools/models | Never the user's personal computer; worker cannot plan new tasks |
| **Worker pool** | Remote capacity associated with cloud or user-supplied remote hosts | Cloud provider and limits are Open |
| **Model assignment** | Model and effort selection per task by the main agent up front | Access entitlement and fallback require validation |
| **CLI mode** | Approved provider's coding-agent CLI running remotely where permitted | Subscription rights for unattended use are not automatically assumed |
| **API mode** | Provider API key integration where permitted and explicitly configured | Different credentials, costs and model capabilities from CLI |
| **Context** | Versioned task/project/plan instructions and validated predecessor refs | Compact/hashed; secrets and untrusted sources excluded from authority |
| **Artifact** | Result file, report or object with immutable hash/provenance | Not validated merely because an executor uploaded it |
| **Validation** | Independent evaluation against task acceptance criteria and artifact provenance | Worker success/exit code cannot directly mark task succeeded |
| **Remote Git ref** | Branch/tag/commit reference on authorized remote Git provider | No personal-computer Git synchronization |
| **Protected merge** | Authorized integration of reviewed remote changes under provider branch rules | Token repository permission does not alone constrain branch writes |
| **Budget ceiling** | Total spending authorization last explicitly approved by a human | No automatic percentage overrun |
| **Reservation** | Conservative held exposure for a proposed billable attempt | Different from predicted cost or settled invoice |
| **Settled spend** | Reconciled provider charge posted to accounting ledger | Can arrive later than worker completion |
| **Unknown exposure** | Billing liability whose bounds/source cannot yet be trusted | Show as uncertain and halt unsafe new admissions |
| **Cost classes** | AI API, cloud compute, storage and transfer | Must remain separately reported |
| **Cost scenarios** | Optimistic, expected and pessimistic forecasts | Forecasts are estimates, **not** approval amounts |
| **Human approval gate** | Explicit authenticated action authorizing an increase in the hard ceiling | A warning acknowledgement or model text is not approval |
| **SQLite** | Rolvya's only authoritative database engine | WAL/driver/backup details are still engineering decisions |
| **ADR** | Architecture Decision Record with status and rationale | Proposed does not mean Approved; approved does not mean Implemented |
| **Implemented** | Source code/service exists at a verifiable commit or deployment | Not necessarily tested |
| **Verified** | Evidence-backed behavior demonstrated by recorded tests | Cannot be inferred from design documents |

## Common misinterpretations

- **20 workers on 4 GB**: old illustrative load, not a measured supported limit.
- **~10 minutes per task**: sizing target, not a mandatory timeout or guaranteed SLA.
- **GitHub App**: a proposed authentication approach under ADR 0008, not installed by documentation.
- **HTTPS worker pull**: a proposed design under ADR 0006, not final protocol approval.
- **Cloud Run / Fargate prices**: region/time/provider dependent; historical examples are not live rates.
- **Rust + SQLite selected**: binding architecture decisions, **not** evidence of a running product.

Related: [vision](../product/vision.md), [scope](../product/scope.md), [orchestrator](../architecture/orchestrator.md), [task model](../architecture/task-model.md), [budget accounting](../architecture/budget-reservation-reconciliation.md), [project status](../project/status.md).
