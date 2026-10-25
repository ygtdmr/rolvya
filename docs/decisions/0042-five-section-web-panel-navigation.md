# ADR 0042 — Five-section navigation for Rolvya's first web panel

Status: **Approved (navigation structure only)**  
Date: 2026-10-25  
Owner decision: the five top-level web panel sections were explicitly accepted in the Rolvya project conversation.

## Context

Rolvya v0.1 already requires a basic role-aware web panel that exposes project execution, results, spending and approvals. The [initial panel structure](../product/initial-web-panel-structure.md) proposed a compact five-section information architecture. The Owner approved that navigation structure without separately approving a frontend framework or detailed screen implementations.

## Decision

The first web panel will have exactly these five **top-level navigation sections**:

1. **Projects** — project overview, remote Git association, fixed hierarchical plan and DAG dependency visibility.
2. **Execution** — task and attempt states, remote worker progress, safe logs and blockers.
3. **Costs & Approvals** — approved ceiling, reserved/actual/uncertain cost, category breakdown and explicit privileged approval workflows.
4. **Connections** — approved remote Git integrations, separate authorized model CLI accounts and API-key connections, and remote worker providers.
5. **Results & Activity** — independently verified artifacts and results, reviewable Git refs, audit/activity history.

These are navigation groupings, not five independently running backend services. A task hierarchy and dependency DAG are distinct concepts, and only approved executable leaves launch workers.

## Approval boundary and preserved safeguards

- The **five names and high-level purpose** are approved. Detailed descriptions and example screens in the linked panel proposal are design guidance, not separate Owner approvals of every widget or workflow.
- Keep previously approved security, role separation, action-bound Owner approvals, optional TOTP, fixed DAG, Rust/SQLite control authority, hard budget stop and separate AI API / compute / storage / transfer accounting.
- Connections must preserve independently isolated multiple Claude/Codex CLI accounts and API-key mode, subject to provider-specific entitlement and secret-handling validation.
- Remote execution is AWS ECS Fargate first, then Google Cloud Run Jobs and enrolled remote VPS per [ADR 0041](0041-aws-fargate-first-real-worker.md); this is neither a new cloud expenditure authorization nor a requirement to deploy all providers for the first working version.
- No local/personal-computer worker, local Git synchronization or credential-file upload is authorized.

## Alternatives and rationale

The initial panel could have used one large undifferentiated dashboard or many separate operational pages. A compact five-section navigation was accepted to keep essential project, execution, financial, connection and evidence functions understandable without prescribing low-level UI organization.

## Open decisions

**Later ADR 0043 approves React + TypeScript, Vite, React Flow, Rust Axum REST/SSE, single application JS containing CSS, and browser/accessibility/i18n requirements.** These choices are **closed**. Exact visual design is deferred by the Owner; code, routing, and browser certification are implementation work, not a reason to revisit framework selection. No authentication rule, screen implementation, feature completeness or visual design is implied by this decision.

## Validation and implementation state

This ADR records a documentation-level product approval. There is no verified running panel, end-to-end UI test, production deployment or completed interaction design. The approval can be checked by verifying the five sections are represented in future designs while UI authorization is still enforced server-side.

Related: [Panel information architecture](../product/initial-web-panel-structure.md), [Product scope](../product/scope.md), [Open decisions](../project/open-questions.md), [Project status](../project/status.md), [Approved AWS-first rollout](0041-aws-fargate-first-real-worker.md).
