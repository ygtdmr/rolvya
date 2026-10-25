# ADR 0048 — Owner-delegated cohesive UI design, Element system and motion quality

Status: **Approved delegation and product design direction; actual UI implementation unverified**  
Date: 2026-10-25  
Owner instruction: The assistant owns detailed Rolvya UI design choices. Make the product distinctive, interconnected (not separately designed screens), highly responsive and optimized; emphasize sophisticated purposeful animation/transitions, current CSS features, and building/reusing **UI Elements first**. Add missing UX and quality requirements proactively.

## Decision

1. **The user has expressly delegated detailed UI design direction to the design/implementation agent.** The previous visual-design deferral in [ADR 0043](0043-single-bundle-browser-platform-and-ux-boundary.md) is superseded **only with respect to authority to select/document visual style, fonts, icons, motion and UI design tokens**. Repeated Owner micro-approval for individual aesthetics is no longer required. This does not authorize replacing the previously approved logo or brand identity.
2. Adopt the unified **Rolvya Connected Control** visual and motion approach specified in the [single authoritative product design contract](../product/connected-control-design-system.md): calm navy/blue/gray technical control surfaces, clear verified signals and connection-path continuity, distinctive but restrained accents, full light/dark and high-contrast behavior. The token palette represents app UI choices; it **does not assert historical logo's unknown exact HEX colors**. No gold branding.
3. **Build shared UI Elements before feature screens.** Tokens → accessible primitives/Elements → reusable orchestration patterns → exactly five approved sections. All pages reuse the same component API, project/task context, semantics, interaction states, motion tokens, layout/density and theme variants. No independent per-screen visual design systems.
4. Use purposeful CSS-first microinteractions, context-preserving view/inspector transitions, verified progress/graph signal effects, carefully bounded timing and consistent easing; prevent infinite decorative motion, excessive layout work and animation from substituting for financial/security confirmation. `prefers-reduced-motion`, focus restoration and functional fallbacks are mandatory.
5. Pursue modern CSS with progressive enhancement: responsive container queries, logical properties, CSS layers/nesting, fluid sizing, modern color composition and carefully capability-tested view transitions, `@starting-style`, anchor positioning, content-visibility and scroll-driven animation. No unrecoverable dependency on partial browser support. Test current Chrome/Firefox/Safari and mobile browsers.
6. **Deep responsive design** is a single Element-system responsibility: from compact phones through tablets/large desktops, 400% zoom, RTL, touch, keyboard and screen readers. The task hierarchy remains independently accessible from the read-only React Flow DAG.
7. **Optimization is an acceptance gate**, not a claim: measure browser real-world Core Web Vitals, JS parse/transfer costs, large DAG/list re-rendering, animation frame drops, SSE update pressure, accessibility and visual regressions. Prefer selective native CSS and shared elements over heavy animation packages. Preserve all strict bundle/CSP/PWA constraints from ADR 0043.
8. **Font/icon asset design selection** is delegated: start with **Inter** for product typography and **IBM Plex Mono** for technical data; use reviewed **Lucide external SVG URL assets** for generic actions. Files remain external—not embedded as JS/CSS payloads or inline icon components—per ADR 0043. Preserve required licenses. **Use the original approved Rolvya logo** when a verified asset is available; do not invent a replacement.
9. Keep existing technical stack, five sections and security intact: [ADR 0042](0042-five-section-web-panel-navigation.md), [ADR 0043](0043-single-bundle-browser-platform-and-ux-boundary.md) React/Vite/React Flow + Rust/Axum REST/SSE, one minified external app JS containing compiled CSS, standalone `sw.js`, external font/icon assets, project-scope permissions, immutable plan, hard budget approval, remote-only work and approved Secret Broker architecture.
10. This owner instruction requests **documentation of the visual design system**, not immediate creation of frontend code, live account connections or paid resources. The previously paused frontend source work is **not silently marked started or finished**. Implement UI after applicable coding work is scheduled.

## Implementation and verification

The authoritative detailed spec is [Connected Control](../product/connected-control-design-system.md). Implement from its U0–U5 Element/design/motion quality gates, together with [the existing F0–F4 browser build plan](../development/web-frontend-source-and-build-plan.md), and pass UI-001–UI-015, AC-153–164, and relevant security/cost acceptance criteria before any completion claim.

**Documentation approval is not a runnable UI, a license/brand-asset audit, a passed cross-browser test or a build benchmark.**

## Consequence and supersession boundary

This settles **who may design the UI and the design-system direction**. It modifies **ADR 0043's earlier “visual design deferred” status only**, not the binding bundling/output contract or app security. The agent may refine semantic design tokens after actual contrast/browser measurements, centrally and with evidence, rather than repeatedly reopening the design choice. Existing unverified brand logo source must be resolved from genuine prior files before branded release.

Related: [brand positioning](../product/brand-positioning.md), [UI Elements architecture](../product/connected-control-design-system.md), [product acceptance](../product/acceptance-criteria.md), [current status](../project/status.md).
