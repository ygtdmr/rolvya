---
title: First-Version Web Panel Structure
status: Approved navigation, stack and delegated unified design; implementation pending
category: product
last-reviewed: 2026-10-25
tags: [web-ui, owner-panel, user-experience, mvp]
---

# Rolvya first-version web panel — approved navigation and proposed details

**Owner-approved:** five top-level navigation sections ([ADR 0042](../decisions/0042-five-section-web-panel-navigation.md)) plus React/TypeScript, Vite, React Flow, REST/SSE through Rust Axum, single minified app JS containing CSS and comprehensive relevant browser support ([ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md)). Fine-grained screen behavior is implementation guidance, not all individually approved. **Subsequent ADR 0048 delegates the visual design** to the design agent and the [Connected Control contract](connected-control-design-system.md) selects the common design language, UI Elements, Inter/Plex typography, external Lucide glyphs, modern CSS and integrated motion across all five sections. This replaces the previous aesthetic deferral without authorizing frontend code/deployment, altering the approved logo, or relaxing browser/security constraints.

## Primary navigation (five sections approved; screen details proposed)

1. **Projects:** List and create projects; show status, associated remote Git repository, blockers and a simple progress/cost summary. Each project's primary view displays the planned **hierarchical task tree AND true DAG dependencies**, which are distinct. Selecting a task opens goals, assigned model/effort/mode, permitted connection, input artifacts, acceptance checks and related attempts.
2. **Execution:** For a selected project, show dependency-ready, running, waiting, paused, failed and verified tasks; per-attempt worker/provider identity, time, redacted logs, retry/fencing state and validation evidence. Make it obvious that a group parent is not an executable worker task. Show when the run is blocked by an unverified provider subscription permission, unavailable worker pool or budget.
3. **Costs & Approvals:** Project-approved spending ceiling, actual/estimated/reserved/uncertain spend, four categories (**AI API, remote compute, storage, data transfer**) and optimistic/expected/pessimistic forecasts. Display paused new billable work with an explicit **fresh Owner-authenticated** budget increase flow; closing an alert is not authorization. Show pending high-risk role/provider decisions with precise scope and existing approvals.
4. **Connections:** Authorized remote GitHub App repositories; **multiple independently authenticated Claude/Codex CLI accounts** (connection identity, permission, reconnect/revoke state), independent API keys through the trusted Secret Broker; remote pools (first working version: AWS ECS Fargate; later Google Cloud Run Jobs and enrolled remote VPS). A disabled/unverified provider mode is never presented as usable. No Local Agent, personal-computer Git or credential-file upload.
5. **Results & Activity:** Verified task outputs, tests, reviewable remote Git refs, error explanations and project Activity Log. Use role-based access and redaction. Do not mark a failed or unvalidated worker output as completed just because a process exited successfully.

## Core user journey

```mermaid
flowchart TD
  A[Create/select project] --> B[Connect permitted remote Git and model access]
  B --> C[Set constraints and authorized budget ceiling]
  C --> D[Planner produces complete DAG and model/effort/mode assignments]
  D --> E[Show plan, estimated costs and known blockers]
  E --> F{All required execution approvals and policy gates satisfied?}
  F -->|No| G[Hold without billable dispatch]
  F -->|Yes| H[Atomic SQLite reservation and remote worker execution]
  H --> I[Validate outputs and publish reviewable results]
  I --> J[Show evidence, Activity Log and reconciled costs]
```

**Important approval boundary:** An explicit budget increase always requires a current verified Owner. Whether **every** first plan activation also needs separate Owner confirmation beyond approved policy remains OQ-013/P1, not assumed in this proposal.

## Role-aware presentation

Owner manages approved critical budget/permissions/connection scope with action-bound verification; Editor may inspect and start already-authorized work when otherwise allowed; Viewer reads only authorized project data. UI visibility does not replace server-side checks.

## User experience priorities

- Readable status before fancy animation; distinguish **planned DAG node**, **worker attempt** and **verified result**.
- Show blocker causes, pending human action and uncertain cloud invoices explicitly.
- The same project dashboard can show real AWS Fargate work while Google Cloud Run Jobs and remote VPS adapters remain under development, consistent with [ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md).
- Accessible keyboard/navigation design and responsive layout; do not interpret responsive web as a commitment to a native mobile app.
- Reuse the approved navy/blue/gray Rolvya identity; no gold accent, no invented replacement logo.

## Approved core technology and browser delivery contract

[React/TypeScript + Vite, React Flow and Rust Axum REST/SSE](../architecture/web-panel-technical-stack-proposal.md) are Owner-approved at the architecture level ([ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md)). Production application JS must be a **single minified file containing compiled CSS**, while **`sw.js`** and manifest/icons are separate assets. Never inline font/icon binaries or glyphs. Refer to the [browser artifact checklist](../architecture/web-platform-artifact-and-capability-contract.md) and [browser UX/accessibility requirements](web-ux-and-accessibility-contract.md). No build/implementation is claimed.

## Still open after navigation approval

Sidebar position/size, exact dashboard widgets, component library, REST/SSE schema and reconnect policy, responsive breakpoints, graph layout, interaction details, animation, filter/search behavior, per-screen final copy, selected font/icon provider, UI aesthetics and first plan activation UX remain for design/implementation review. **WCAG 2.2 AA is the accessibility target**, with exact verified test evidence pending. Do not reopen approved React/Vite/React Flow/REST+SSE or single-app-JS output constraints by calling them undecided.

Related: [approved v0.1 scope](scope.md), [existing user journeys](user-flows.md), [budget hard stop ADR](../decisions/0002-hard-budget-approval.md), [roles ADR](../decisions/0024-project-human-roles.md), [current provider order ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md), [approved five-section navigation ADR 0042](../decisions/0042-five-section-web-panel-navigation.md), [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), [remaining OQ-013](../project/open-questions.md).

This is **not** a completed, implemented or visually reviewed UI.
