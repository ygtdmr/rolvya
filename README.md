# Rolvya

**One Plan. Many Agents.**

Open-source orchestration for AI coding agents, with dependency-aware task planning and parallel execution.

> [!IMPORTANT]
> **Project status: Rust core foundation.** Typed domain primitives and a deterministic immutable-plan validator are implemented as libraries; see [implementation and verification](docs/development/rust-core-foundation.md). The orchestration capabilities below remain **design goals**, not claims of a running product. There is no production-ready release or supported installation procedure yet.

## What is Rolvya?

Rolvya aims to coordinate multiple AI coding agents around a shared, explicit plan. Instead of independently assigning unrelated prompts, an orchestrator will break a software project into tasks, represent their dependencies, and schedule independent work in parallel when safe.

Rolvya is intended to be open-source, extensible, and transparent about execution state, decisions, and costs.

## Design goals

- **Plan before execution:** define a task graph with explicit dependencies.
- **Parallelize safely:** allow independent tasks to run concurrently, while respecting dependencies.
- **Coordinate agents:** distribute work across coding agents and track task ownership, outputs, and completion.
- **Control costs:** estimate AI API, cloud worker, storage, and transfer costs using optimistic, expected, and pessimistic scenarios; require explicit approval before exceeding the remaining project budget.
- **Recover and review:** preserve completed work and support inspection, retries, and human oversight.
- **Document decisions:** keep architecture, interfaces, decisions, and implementation status accessible to both people and coding agents.

These are planned capabilities. Their interfaces and implementation details will be established and documented as development progresses.

## Confirmed technology decisions

- **Backend:** Rust only for Rolvya-owned services and backend logic ([ADR 0004](docs/decisions/0004-control-plane-language.md)).
- **Authoritative database:** SQLite only, accessed through the Rust backend ([ADR 0005](docs/decisions/0005-authoritative-database.md)).

Also **Approved**: worker-initiated HTTPS ([ADR 0006](docs/decisions/0006-worker-communication.md)), GitHub App authentication ([ADR 0008](docs/decisions/0008-github-authentication.md)) and extensible multi-provider remote workers ([ADR 0011](docs/decisions/0011-multi-provider-worker-infrastructure.md)). The [approved first real worker is AWS Fargate](docs/decisions/0041-aws-fargate-first-real-worker.md); the Rust core libraries are now implemented. Worker HTTPS wire/auth and live integration remain future implementation work. These architecture approvals are **not yet implemented functionality**.

## Core concepts (planned)

| Concept | Intended role |
| --- | --- |
| Orchestrator | Coordinates the overall plan, task lifecycle, and agent work. |
| Task DAG | A directed acyclic graph encoding tasks and their prerequisites. |
| Coding agents | Perform scoped coding tasks according to the plan. |
| Scheduler | Identifies ready tasks, manages parallel execution, and respects resource and budget constraints. |
| Human approval | Handles decisions and hard budget gates that must not be bypassed automatically. |

## Architecture principles

Rolvya will use upfront, dependency-aware task planning. Tasks must not begin until required predecessors are satisfied. Independent tasks may execute in parallel where doing so does not compromise consistency or safety.

**Budget policy:** insufficient remaining budget pauses new billable work. Any budget increase requires explicit user approval; there is no automatic percentage overrun. Completed work must be preserved.

**Repository integration:** Git collaboration should operate through remote repositories and services. Direct access to, or synchronization with, a user's personal computer is outside the project scope.

Detailed technical designs, contracts, and architecture decision records will be maintained separately as they are finalized. This README is an entry point, not a substitute for the architecture specification.

## Project status

**Current phase:** initial repository setup and specification.

The proposed capabilities, module boundaries, and technical architecture must be distinguished from implemented and tested functionality. As development begins, this section will be updated with verifiable milestones and links to the relevant code and documentation.

## Documentation

The project will develop a structured documentation set covering:

- Product requirements and scope
- System architecture and component responsibilities
- Task graph, scheduling, and agent coordination
- Execution lifecycle, failure recovery, and security
- Budget enforcement and cost estimation
- Interfaces, data models, and deployment
- Architecture decision records (ADRs)
- Agent development instructions, implementation handoffs, and current project status

Start with the [documentation index](docs/README.md), [current project status](docs/project/status.md), [decision register](docs/decisions/README.md), and [documentation compatibility policy](docs/development/documentation-policy.md). The [architecture overview](docs/architecture/overview.md) links to existing design documents. Further documents remain planned; implementation agents must not mistake proposals for working features.

## Roadmap (high level)

1. **Foundation:** repository standards, project specifications, decision records, and agent instructions.
2. **Architecture:** define component contracts, task lifecycle, scheduler semantics, and budget gates.
3. **Initial implementation:** build and test a minimal dependency-aware orchestration workflow.
4. **Integrations and reliability:** add supported agent integrations, remote repository workflows, observability, and recovery.
5. **Hardening:** security reviews, end-to-end tests, documentation, and release preparation.

This roadmap is directional, not a delivery commitment.

## Contributing

Rolvya welcomes future contributions and technical discussion. Contribution guidelines, coding conventions, test requirements, and issue templates will be added during repository setup. Until then, please open an issue to discuss substantial changes before starting work.

**Agent-assisted development:** the project is intended to be implemented through AI coding agents under human direction. Agents should work from documented requirements, record assumptions and decisions, update documentation alongside code, and clearly report what was implemented and tested.

## Security

Security reporting instructions and a disclosure policy will be published before release. Until then, do not publish sensitive exploit details, credentials, or secrets in public issues.

## License

Rolvya is licensed under the **MIT License** (SPDX: `MIT`). See [LICENSE](LICENSE) for the full text.

---
