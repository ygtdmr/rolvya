# Instructions for AI development agents

Rolvya is an AI-agent-orchestration project developed by AI coding agents under human direction. Read this file first.

## Required reading order
1. [Documentation index](docs/README.md)
2. [Project status](docs/project/status.md)
3. [Decision register](docs/decisions/README.md)
4. [Documentation and Obsidian compatibility policy](docs/development/documentation-policy.md)
5. [Engineering standards](docs/development/standards.md), [agent handoffs](docs/development/agent-handoffs.md) and [review process](docs/development/review-process.md)
6. [Open design decisions](docs/project/open-questions.md)
7. Relevant product, architecture, and operations specifications
8. Source code and tests (when present)

## Non-negotiable rules
- Implement every **Rolvya-owned backend service or backend logic in Rust only**; see [approved ADR 0004](docs/decisions/0004-control-plane-language.md). Do not add Go, Python or Node.js/TypeScript backend services without a new explicit owner-approved ADR. User-project tools running on remote workers are outside the Rolvya backend implementation rule.
- Use **SQLite as the only authoritative database engine**, through the Rust backend; see [approved ADR 0005](docs/decisions/0005-authoritative-database.md). Do not introduce PostgreSQL or another alternate source of truth without explicit owner approval. Specific SQLite tuning, SQL schema and migration tooling remain proposed.
- Never represent a proposal, design goal, or unimplemented feature as working code.
- Do not silently resolve undecided technology, architecture, security, or product questions. Record the question and request a human decision when consequential.
- Preserve approved decisions. Changes require documented rationale and explicit owner approval.
- Do not add a Local Agent, Local Sync, or direct access to/synchronization with Git repositories on the user's personal computer. Remote Git services are the integration scope.
- A project budget is a **hard gate**: when insufficient, pause new billable tasks and seek explicit user approval for an increase; do not automatically permit an overrun. Preserve completed work.
- Distinguish estimated AI API, cloud worker, storage, and data-transfer costs. Keep optimistic, expected, and pessimistic estimates distinct.
- Never expose credentials, secrets, or sensitive user data in commits, logs, test fixtures, or agent messages.
- Do not represent an unresolved question in [the open-decision register](docs/project/open-questions.md) as an approved implementation choice.
- Perform work in reviewable, scoped changes. Add or update tests for behavior changed, and document what was actually tested.
- Synchronize documentation with implementation; update the project status and impacted specifications or ADRs in the same change.
- Follow [the documentation policy](docs/development/documentation-policy.md): GitHub-compatible relative Markdown links, optional portable YAML frontmatter, Mermaid where helpful, and Obsidian-friendly organization.
- Do not invent endpoints, infrastructure deployments, integration credentials, or completed milestones.

## Task handoff requirements
Every substantive agent handoff must state:
- Objective and scope; relevant requirement/issue/ADR links
- Files and modules changed; branch or commit identifiers
- Decisions made, assumptions, and unresolved questions
- Tests run, exact results, and tests not run
- Known limitations, risks, budget impact, and next recommended tasks

## Source of truth
- Approved decisions: `docs/decisions/`
- Requirements and project scope: `docs/product/`
- Component designs: `docs/architecture/`
- Actual delivery state: `docs/project/status.md`
- Agent-wide working protocol: this file

If documentation and code disagree, inspect the discrepancy and report it. Do not claim that either is correct without verification.
