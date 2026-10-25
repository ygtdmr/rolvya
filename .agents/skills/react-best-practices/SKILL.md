---
name: react-best-practices
description: Review and implement efficient React and TypeScript components in Rolvya's Vite-based control panel. Trigger on rendering, state, fetching, bundle or frontend performance work; exclude Next.js-only guidance.
---

# React best practices — Rolvya/Vite

Scoped, self-contained Rolvya adaptation informed by [Vercel React Best Practices](https://github.com/vercel-labs/agent-skills/blob/063bee94c3f4df8453406c830b0a7df0f2860278/skills/react-best-practices/SKILL.md) (upstream declares MIT; revision `063bee94c3f4df8453406c830b0a7df0f2860278`). No upstream scripts, network fetch requirement, frameworks or packages are installed.

## Start with Rolvya's own contract

Read `AGENTS.md`, ADR 0043 (single minified app JS with embedded CSS), ADR 0048 and the Connected Control design system. React + TypeScript + Vite + React Flow are approved; **Next.js, RSC, server actions and new Node backends are not**. Use Graphify to find existing consumers before changes and refresh it after modifying code.

## Performance review order

1. **Dependency waterfalls:** launch independent fetches concurrently only when safe; keep budget/permission-denied state explicit. Use the approved Axum REST + SSE interface rather than adding an alternative backend.
2. **Bundle invariants:** avoid broad barrel imports and accidental external chunks. Direct imports, module boundaries and conditional behavior must still pass Rolvya's exactly-one-app-JS-and-embedded-CSS production artifact check. Never apply a lazy import that splits production output into disallowed JS chunks.
3. **Re-renders:** keep state close to consumers; derive data in render when possible; memoize only measured expensive computations; use stable keys; virtualize very large DAG/list surfaces while preserving accessible alternatives.
4. **Rendering:** use CSS for scalable motion, reduce layout thrashing, preserve keyboard focus and touch targets; respect `prefers-reduced-motion` and browser fallback behavior.
5. **Data fetching:** handle stale/failed/retrying network states and SSE reconnects; do not expose secrets; avoid cross-project caching or stale authority information.
6. **Testing and measurement:** run lint/typecheck/tests, real production build and artifact inventory; measure the actual user interaction rather than claiming optimization from a generic checklist.

If a rule in the upstream catalog assumes Next.js, SSR, RSC, automatic CSS splitting or unsafe CSP inline script, it is **inapplicable**, not permission to change the architecture. Prefer the approved Vite frontend contract.
