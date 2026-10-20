# Decision register

This register separates **approved user decisions** from **earlier design concepts** and unresolved choices. It is not a claim that the platform has been implemented.

## Confirmed decisions (2026-10-20 checkpoint)
| ID | Status | Decision | Implication |
| --- | --- | --- | --- |
| D-001 | Approved | Rolvya name, accepted logo and navy/blue/gray visual identity; no redesign requested. | Preserve brand identity. |
| D-002 | Approved | Public `ygtdmr/rolvya` repository, default branch `main`. | GitHub-hosted project source of truth. |
| D-003 | Approved | GNU MIT license. | Do not silently replace with MIT or another license. |
| D-004 | Approved | "Open-source orchestration for AI coding agents, with dependency-aware task planning and parallel execution." | Approved short description. |
| D-005 | Approved | "One Plan. Many Agents." tagline. | Use sparingly; not repeatedly in one document. |
| D-006 | Approved | Upfront dependency-aware DAG task planning and parallelization of independent tasks. | Scheduler must enforce dependencies. |
| D-007 | Approved | Entire coding implementation by AI agents, under human architectural and priority direction. | Agent-continuable, complete documentation and evidence-based handoffs. |
| D-008 | Approved | Budget is a hard approval gate. | Pause new billable tasks when funds insufficient; require explicit increase; preserve completed work; no percentage overrun. |
| D-009 | Approved | Cost planning separates AI API, cloud workers, storage and transfer, with optimistic/expected/pessimistic scenarios. | Design estimators around these categories. |
| D-010 | Approved | No Local Agent, Local Sync, or direct access/synchronization with Git repositories on user's personal machine. | Support remote Git services rather than personal-computer access. |

## Previously explored architecture; validate before treating details as binding
- Main orchestrator decomposes hierarchical tasks, selects AI models and effort levels, manages queue, dependencies and contextual memory.
- Worker scheduler considers priority, duration, CPU/RAM, model constraints, capacity, costs and region/latency.
- Cloud worker candidates included AWS ECS Fargate, Google Cloud Run, and separately managed remote servers; a Rust orchestrator on a small VPS was depicted in an earlier diagram.
- Planned flow: queue → parallel workers → results and parent-task aggregation → final tests and validation.
- Candidate operational concerns: worker lifecycle, health, authentication, retries, logs, metrics and cost reporting.
- An earlier diagram referenced a local VPS and an illustrative 20-worker limit. These are **not confirmed production limits**, and a VPS-hosted orchestrator does **not** authorize a personal-computer Local Agent.

## Outstanding decisions / research
- Detailed product flows, concrete component boundaries, Rust and UI stack commitment, persistence and data isolation.
- Graph/task contracts, concurrency policy, worker adapters, failure/retry/idempotency behavior.
- Provider choice, infrastructure and credential models.
- Concrete authentication/authorization, secrets, threat model, API schemas, budget enforcement and approval UX.
- Roadmap milestones, test strategy, support and release process.
- Legal/commercial implications of MIT and a separate future trademark policy. MIT does not generally prohibit commercial competing use; brand restrictions must not be falsely described as MIT requirements.

## ADR process
Create `NNNN-short-decision-title.md` for consequential decisions. Include context, options, chosen decision, owner/approval date, consequences, rejected alternatives, related requirements, migration notes and supersession links. Never mark an item approved just because an earlier assistant suggested it.

Review this register against original conversations and diagrams before claiming historical coverage is complete.
