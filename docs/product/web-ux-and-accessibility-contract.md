---
title: Rolvya Web User Experience and Accessibility Contract
status: Approved UX principles and subsequent delegated unified design; implementation open
category: product
last-reviewed: 2026-10-25
tags: [ux, accessibility, i18n, pwa, user-flows, browser]
---

# Rolvya browser UX and accessibility contract

## Subsequent design delegation — ADR 0048

The Owner subsequently delegated UI aesthetics, typography/icons, animation, latest CSS use, responsiveness and the integrated UI Elements catalog to the design agent. [ADR 0048](../decisions/0048-owner-delegated-cohesive-ui-design.md) and [Connected Control](connected-control-design-system.md) **supersede the earlier visual-design deferral below**, not this document's security, keyboard, comprehension, role or accessibility requirements. Do not treat this historic "do not select" paragraph as a live ban on the now-delegated design. No screen or browser test has been run.

## Approval scope

[ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) authorizes defining UX behavior and browser accessibility without settling **UI aesthetic/design decisions**. The five top-level panel navigation sections are already Approved in [ADR 0042](../decisions/0042-five-section-web-panel-navigation.md). These UX principles are requirements for future implementation/validation, not proof of a working UI.

**Do not select:** font families or font source, icon provider/icon families, spacing, visual compositions, imagery, theme variations, specific animation, colors beyond existing approved brand identity, component/styling libraries or bespoke logo production. Such choices are deferred to the implementation/design stage with review.

## UX behavior by five approved sections

| Section | User task and UX contract |
| --- | --- |
| **Projects** | Find/create/select a project; see its remote Git source, goal and stable plan version/hash; accessible separate hierarchy and dependency DAG; current blockers and concise cost status; inspect a planned leaf's role, model/effort, permissions and acceptance evidence without leaking secrets. |
| **Execution** | Understand *ready vs blocked vs running vs pending-validation vs verified vs failed vs paused vs unknown*; identify each distinct attempt, provider identity and timestamps; inspect redacted logs; do not treat job exit as verified acceptance. Show what is happening without implying that a retry added DAG tasks. |
| **Costs & Approvals** | View approved ceiling, reserved, actual, provisional and unknown exposure; separate AI API/compute/storage/transfer and optimistic/expected/pessimistic forecasts; budget pause has visible reason/action. Display exact proposed change, associated project and cost impact before authenticated one-use Owner authorization; 'dismiss' never equals 'approve'. |
| **Connections** | Manage remote GitHub App grants, independent named Claude/Codex CLI accounts, separate API credentials and worker pool capabilities. Show verified/expired/revoked/unsupported/entitlement-unknown status. Never silently switch accounts, API vs CLI mode, provider or incur extra expenses. No Local Agent/Sync or browser credential-file upload. |
| **Results & Activity** | Trace accepted result to exact task, attempt, version, test/evidence and authorized Git ref. Present audit events chronologically, with filtering, stable pagination and readable error explanations; keep failed output visibly unverified. |

## Core experience principles (implementation acceptance guidance)

1. **System status and provenance:** Every material operation exposes a comprehensible pending/success/failure/unknown state. 'Connection lost' is not 'worker failed'; 'worker finished' is not 'result verified'; 'cost not yet billed' is not zero. Add visible last-sync timestamp and manually triggerable refresh if event stream is stale.
2. **Prevent expensive mistakes:** Preflight shows a complete already-planned DAG, authorization blockers and the three forecast scenarios before execution. Plan activation consent policy remains explicitly **open**; the existing approved requirement for fresh, transaction-bound Owner proof on budget/privileged changes always applies.
3. **Explain why blocked:** Each disabled action has a machine-independent reason: unapproved budget, missing dependency, missing entitlement, invalid connection, session reauthentication, provider unavailable, or unsafe billing exposure. Provide non-billable resolution choices where applicable.
4. **Keep tasks discoverable:** Hierarchy is a separate readable tree; DAG is an inspectable graph; both select the same task and preserve keyboard-visible focus and stable IDs. Avoid relying solely on color or node position to communicate dependencies/status. Provide non-graph tabular/list alternatives.
5. **Recoverable navigation:** Stable routes, browser Back/Forward, direct links with permission checks, deterministic refresh behavior, focus restoration, pending changes confirmation only when actually at risk of loss, no inaccessible modal traps.
6. **Offline/disconnected experience:** The authenticated dashboard must not pretend to have fresh finances or perform privileged actions offline. Safe shell and explanatory retry UI may remain accessible. Never queue or auto-replay billable launches, plan approvals, role changes or sensitive mutations.
7. **Transparency without notification overload:** Quiet by default. User-relevant notifications only when authorized and meaningful, with in-panel history and optional opt-in browser push where supported. Notification permission is never requested automatically.
8. **Accessible verification:** WCAG 2.2 **AA is the target**, including keyboard operation, clear accessible names, visible focus, status announcements, meaningful validation error text, adequate contrast (tested after actual UI design), zoom/reflow and reduced motion. Tests include screen-reader and keyboard flows; automatic audit alone is insufficient.
9. **Internationalization ready:** Locale negotiation and explicit override; use `Intl` for numeric/date/time/currency units, preserve exact money values independent of locale, correct pluralization, `lang`/`dir`, RTL layout and pseudolocalization, no clipped action labels. Actual shipping languages and translation text require a separate decision.
10. **Browser progressive enhancement:** Pointer/touch/keyboard input, high-contrast and OS/browser preferences, installable PWA capabilities where available, graceful denied-permission/unsupported-API fallback. Device sensors, microphone, camera, background sync or file-system capabilities are only introduced for specific approved flows.
11. **Security clarity:** Clearly mark active account/project, role and connected credential identity; display relevant change targets; handle expired session without exposing data; only Rust backend authorizes work. Never display broad credentials or treat local UI state as financial authority.
12. **Responsive and low-resource:** Functional small-screen views, accessible desktop multitasking, efficient event refresh, no mandatory hover/drag. Exact breakpoints, animations, layout and typography are **not** chosen yet; measure actual performance before claims.

## Cross-cutting acceptance matrix (future tests)

| Scenario | Expected observable UX |
| --- | --- |
| Two independent leaves run while dependent leaf is blocked | Two attempts shown independently, prerequisite reason and final validation shown, immutable DAG unchanged |
| Provider CLI account unauthorized or expired | Named connection marked unusable, actionable explanation, no account or API fallback |
| Insufficient reserved budget | Visible project pause and remaining limit, no new paid launch, exact Owner authorization path |
| Already running task with uncertain final charge | Provisional/unknown costs visibly separate from finalized costs; no falsely free attempt |
| Stale SSE event or service worker offline | Last-updated indicator and safe revalidation, no false 'completed' or paid replay |
| Browser without PWA install | Full normal authenticated web experience, no broken permission prompt |
| Keyboard-only user on complex task graph | Searchable/linear alternative, inspect nodes and edges, focus never trapped |
| RTL pseudo-locale or 200% zoom | Correct order, no clipped critical controls, labels remain meaningful |
| Font/icon CDN blocked | Readable system-font fallback and labelled text controls, no inaccessible icon-only actions |
| Expired human session during Owner approval | Sensitive action held, clear reauthentication, atomic approval not applied by stale screen |
| Reduced motion or screen reader | No essential information hidden in animation/color, important state changes announced appropriately |

## Deferred detailed-design decisions

Visual UI treatment (including typography and icon provider), concrete component/routing packages, locale list, responsive dimensions, action shortcut mappings, event cadence, push implementation and graph auto-layout are still subject to explicit implementation choices. This document does **not** unilaterally decide whether every initial plan activation needs separate Owner confirmation. Keep previously approved budget and security boundaries.

## References

- [WCAG 2.2](https://www.w3.org/TR/WCAG22/).
- [WAI accessibility standards](https://www.w3.org/WAI/standards-guidelines/wcag/).
- [MDN Web application manifest](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest).

Related: [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), [browser build and asset contract](../architecture/web-platform-artifact-and-capability-contract.md), [panel information architecture](initial-web-panel-structure.md), [identity](../architecture/identity-authorization.md), [approval workflow](../architecture/approval-workflow.md).
