---
name: web-design-guidelines
description: Audit Rolvya frontend code for accessible interactions, consistent Connected Control visuals, responsive modern CSS and UX correctness. Trigger on UI reviews, layout bugs, focus problems and motion/accessibility work.
---

# Web interface review — Rolvya

Scoped self-contained review method informed by [Vercel web-design-guidelines](https://github.com/vercel-labs/agent-skills/blob/063bee94c3f4df8453406c830b0a7df0f2860278/skills/web-design-guidelines/SKILL.md) (upstream MIT declaration; revision `063bee94c3f4df8453406c830b0a7df0f2860278`). No live fetch of changing guidelines or mandatory Vercel tooling.

## Review protocol

1. Use the user's specified files, or locate the changed frontend files and their shared UI Elements in the PR. Query Graphify for component consumers first, then read source.
2. Check semantic controls, ARIA names, labels and errors, predictable tab order, visible focus, dialog focus restoration, keyboard equivalents for mouse gestures, and correct disabled/loading state.
3. Check contrast and WCAG 2.2 AA targets, responsive layouts from narrow phones to large desktops, minimum touch targets, zoom, high-contrast mode, `prefers-reduced-motion`, i18n/RTL and long-content resilience.
4. Ensure unified token usage, typography, icons, motion durations and reusable Elements per the approved Connected Control design system. Do not introduce a second aesthetic or alter the existing logo.
5. Check responsive `@container`, progressive `@supports`, `content-visibility`, `clamp()` and View Transitions only where browser support and tested fallback are suitable.
6. Inspect loading/empty/error/stale/offline and reduced-permission states for each affected screen, including alternative accessible DAG tree navigation. Compare actual behavior with screenshots/browser checks when possible.
7. Verify one app JS with CSS embedded, approved standalone assets, strict CSP and no unsafe runtime asset injection using production build and artifact inspector; do not assert this from source alone.
8. Return prioritized findings with `file:line`, severity, evidence, reproducible steps and a proposed test. Separate verified failures from suspicions and clearly mark inaccessible browser tests.

Approved Rolvya contracts outrank generic web advice. This skill is a reviewer, not a deployment tool or permission to make architectural decisions.
