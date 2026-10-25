---
name: rolvya-connected-ui
description: Build or review Rolvya's cohesive React/TypeScript control panel, shared UI Elements, accessible DAG views, motion, responsive CSS, and exact single-bundle output. Use for frontend product work; not for backend or standalone marketing designs.
---

# Rolvya Connected Control UI

1. Read `AGENTS.md`, ADRs 0042, 0043 and 0048, `docs/product/connected-control-design-system.md`, and `docs/development/web-frontend-source-and-build-plan.md`. The approved Rolvya logo, navy/blue/gray palette and Connected Control system are binding.
2. Query Graphify for reusable component relationships and route dependencies before touching existing UI. Confirm actual source; run `graphify update .` after edits.
3. Build shared tokens, primitives, Elements, patterns and motion behaviors before creating route-specific versions. Compose the same Elements across Projects, Execution, Costs/Approvals, Connections and Results/Activity. Avoid cloned modals, status indicators or divergent animations.
4. Use React, TypeScript, Vite and React Flow for the read-only activated DAG. Provide a separately accessible hierarchical/list alternative; never conflate task group hierarchy with execution dependencies.
5. Favor thoughtful micro-interactions, high-performance transitions, responsive container queries and modern CSS with supported fallbacks. Respect prefers-reduced-motion, keyboard focus, touch targets, WCAG 2.2 AA, locale/RTL and meaningful loading/error/stale/offline states.
6. Enforce the production artifact invariant with real builds: exactly one minified app JS containing application CSS, with a separate service worker and explicitly allowed static assets; no emitted app CSS/chunks, embedded icon/font payloads, or unsafe CSP workaround. Use external Inter/IBM Plex Mono and Lucide assets as approved.
7. Verify screen reader/keyboard flows, narrow and wide viewports, browser compatibility and actual emitted bundle inventory. Do not claim PWA installation, CSP correctness, performance or accessibility without measured/reproducible tests.
8. Provide screenshots or artifact evidence when available, exact test commands, affected UI Elements, accessibility trade-offs and known platform limitations. Never add a Node backend.

Related documents: `docs/product/web-ux-and-accessibility-contract.md` and `docs/architecture/web-platform-artifact-and-capability-contract.md`.
