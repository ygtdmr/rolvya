# Instructions for AI development agents

Rolvya is an AI-agent-orchestration project developed by AI coding agents under human direction. Read this file first.

## Required reading order
1. [Documentation index](docs/README.md)
2. [Project status](docs/project/status.md)
3. [Decision register](docs/decisions/README.md)
4. [Documentation and Obsidian compatibility policy](docs/development/documentation-policy.md)
5. [Engineering standards](docs/development/standards.md), [agent handoffs](docs/development/agent-handoffs.md) and [review process](docs/development/review-process.md)
6. [Open design decisions](docs/project/open-questions.md) and [historical conversation coverage audit](docs/project/two-conversation-coverage-audit.md)
   - For orchestration changes also consult [orchestrator](docs/architecture/orchestrator.md), [task model](docs/architecture/task-model.md) and [agent contracts](docs/architecture/agents.md).
   - For budget authority consult [approval workflow](docs/architecture/approval-workflow.md). For backend organization consult the [Rust workspace proposal](docs/architecture/rust-workspace.md).
7. Relevant product, architecture, and operations specifications
8. Source code and tests (when present)

## Non-negotiable rules
- Implement every **Rolvya-owned backend service or backend logic in Rust only**; see [approved ADR 0004](docs/decisions/0004-control-plane-language.md). Do not add Go, Python or Node.js/TypeScript backend services without a new explicit owner-approved ADR. User-project tools running on remote workers are outside the Rolvya backend implementation rule.
- Use **SQLite as the only authoritative database engine**, through the Rust backend; see [approved ADR 0005](docs/decisions/0005-authoritative-database.md). Do not introduce PostgreSQL or another alternate source of truth without explicit owner approval. Specific SQLite tuning, SQL schema and migration tooling remain proposed.
- Use **worker-initiated HTTPS pull/claim, heartbeat and result submission** for remote worker control; do not replace it silently with gRPC/SSH/broker. Approved [ADR 0006](docs/decisions/0006-worker-communication.md); wire/schema/auth settings remain open.
- Use **GitHub App installations** for GitHub authentication and a trusted **Rust Git publisher**; worker model contexts must not receive broad installation write credentials. Enforce protected ref policy separately. Approved [ADR 0008](docs/decisions/0008-github-authentication.md).
- Implement **extensible multi-provider Rust remote worker adapters**, not a scheduler locked to one cloud. Initial providers remain unselected; no personal-computer worker is allowed. Approved [ADR 0011](docs/decisions/0011-multi-provider-worker-infrastructure.md).
- Treat every parent/non-leaf task as a **non-executable group**; only planned executable leaves may receive worker attempts. Group success requires verified required children and applicable group acceptance. See [Approved ADR 0009](docs/decisions/0009-parent-aggregation.md).
- A running paid worker may continue only if **full remaining exposure is enforceably bounded and pre-reserved**; otherwise initiate stop-on-risk/fencing and external cost reconciliation. Do not start new billable attempts without approved headroom. See [Approved ADR 0010](docs/decisions/0010-inflight-budget.md).
- For the web panel use the Owner-approved **React + TypeScript / Vite / React Flow** (read-only activated DAG, separate accessible tree) and **Rust Axum REST + SSE** approach. Production assets MUST have exactly **one minified application JS containing CSS**; separate standalone `sw.js`, manifest, external icon assets and optional JSON/localization are allowed. Never emit an extra app JS chunk or CSS, or embed font/icon payloads; verify CSP/PWA/i18n/WCAG 2.2 AA/browser fallbacks. **Do not select visual UI design, font or icon provider without later review.** See [ADR 0043](docs/decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), [browser artifacts](docs/architecture/web-platform-artifact-and-capability-contract.md) and [UX/accessibility](docs/product/web-ux-and-accessibility-contract.md).
- Never represent a proposal, design goal, or unimplemented feature as working code.
- For model access, preserve the historical subscription-only Claude CLI / authorized API-key mode distinction, but **do not assume subscription terms permit third-party unattended cloud orchestration**. Verify [model entitlement](docs/architecture/model-access-entitlement-contract.md) and [dated official provider policy](docs/reference/provider-cli-api-policy-evidence.md), failing closed when uncertain.
- Apply [scheduler eligibility policy](docs/architecture/scheduler-selection-policy.md) before ranking by price/priority. Historical 20 workers, cloud prices, 1/3/10-minute retry delays and three attempts are **illustrations**, never universal configuration. See [retry historical analysis](docs/architecture/retry-escalation-policy.md).
- Preserve the finalized no-gold wordless logo and the **MIT License** (SPDX: `MIT`); do not change the chosen brand or project license without explicit owner approval. See [brand provenance](docs/product/brand-history-license-governance.md).
- Do not silently resolve undecided technology, architecture, security, or product questions. Record the question and request a human decision when consequential.
- Preserve approved decisions. Changes require documented rationale and explicit owner approval.
- Do not add a Local Agent, Local Sync, or direct access to/synchronization with Git repositories on the user's personal computer. Remote Git services are the integration scope.
- A project budget is a **hard gate**: when insufficient, pause new billable tasks and seek explicit user approval for an increase; do not automatically permit an overrun. Preserve completed work.
- Distinguish estimated AI API, cloud worker, storage, and data-transfer costs. Keep optimistic, expected, and pessimistic estimates distinct.
- Never expose credentials, secrets, or sensitive user data in commits, logs, test fixtures, or agent messages.
- Initial credential storage is **Owner-approved authenticated-encrypted secret records inside SQLite**, accessed only by the trusted Rust Secret Broker, **not plaintext SQLite or mandatory AWS Secrets Manager/Vault** ([ADR 0044](docs/decisions/0044-encrypted-sqlite-secret-storage.md)). **ADR 0045 approves automatically loading an existing master key from a separate, OS-restricted local file on the remote controller host**, outside the DB, ordinary backups, source, images and web roots. No manual unlock at each restart. If the key is missing/unreadable/invalid, **fail closed and never regenerate or overwrite encrypted records on ordinary startup**. **ADR 0047 approves XChaCha20-Poly1305, 256-bit OS CSPRNG KEK, independently wrapped per-record DEKs and guarded one-time setup**, not repeated or missing-key creation. Exact crate versions, record format, recovery-kit cipher/KDF, deployment path, staged rotation and remote identity remain **OQ-005/P0**. See [ADR 0045](docs/decisions/0045-automatic-local-master-key-unlock.md).
- Treat the approved [ADR 0047 cipher and first-run key hierarchy](docs/decisions/0047-secret-broker-xchacha20-key-hierarchy.md) as binding. Authenticate context with project/connection/purpose-associated data and reject mismatched ciphertext/key versions without exposing secrets. Never store plaintext KEK/DEK material in SQLite, logs, model prompts, worker images or ordinary backups. The [ADR 0046](docs/decisions/0046-off-host-encrypted-master-key-recovery.md) emergency recovery kit has separate custody and has **not** been implemented.
- Do not represent an unresolved question in [the open-decision register](docs/project/open-questions.md) as an approved implementation choice.
- The [worker wire draft](docs/architecture/worker-wire-contract-draft.md), [Git publication contract](docs/architecture/git-remote-integration-contract.md), [multi-provider adapter contract](docs/architecture/multi-provider-adapter-contract.md) and [remote worker lifecycle](docs/operations/remote-worker-lifecycle-contract.md) remain **Proposed implementation details**. The HTTPS, GitHub App and multi-provider architecture choices **are Approved**; first cloud providers, provider secrets, rollout and precise wire contracts are not. Consult the [decision pack](docs/project/transport-git-hosting-decision-pack.md).
- Perform work in reviewable, scoped changes. Add or update tests for behavior changed, and document what was actually tested.
- For the **first frontend implementation slice**, consult [source/build plan F0–F1](docs/development/web-frontend-source-and-build-plan.md) before scaffolding. All example paths/config are **Proposed** until implemented and tested; never claim CSS embedding, CSP compatibility or PWA installability without a passing production artifact inspector and real browser test.
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
