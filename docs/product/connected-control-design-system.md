---
title: Rolvya Connected Control — Unified UI Design System and Motion Contract
status: Owner-delegated design direction; implementation and visual validation pending
category: product
last-reviewed: 2026-10-25
tags: [ui, design-system, motion, css, responsive, accessibility, performance]
---

# Rolvya Connected Control — the single visual and interaction language

## Authority and scope

**Owner direction — 2026-10-25:** The Owner delegates Rolvya's UI **creative design choices** to the implementation/design agent; requests a **distinctive** high-quality design, unusually polished transitions/animation, current modern CSS features, strong optimization and excellent responsive behavior. **Create shared UI Elements first**, then compose all five screens from them so **every visual and interaction decision is coherent across the product**. The agent should fill in reasonable missing UX details rather than repeatedly requesting micro-approvals.

This design contract makes the visual direction specific enough for implementation **without changing** [ADR 0042's five sections](../decisions/0042-five-section-web-panel-navigation.md), [ADR 0043's browser packaging/security boundary](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), accepted Rolvya logo, approved role/security policies, or the underlying Rust/SQLite authority. **No frontend source files or mock branded logo were created by this document.** Actual deployment, accessibility, performance and browser support are not yet verified.

## Core concept: connected, calm and unmistakably Rolvya

**Design identity: “Connected Control.”** An original, precision-engineered orchestration console: information is spatially connected, visually calm and readable, with small purposeful blue signal/connection accents rather than generic gradient-heavy SaaS cards. The core visual metaphor is **a system of paths, nodes and verified signals**. Connection lines, card edge treatments, selected paths, progress indicators and transitions share the *same* geometry, hierarchy, color and timing. This does **not** replace the approved wordless geometric Rolvya mark with a new one.

Principles:
1. **One system, many views:** one token catalog, one component API, one motion language, one set of status meanings across Projects, Execution, Costs & Approvals, Connections, Results & Activity.
2. **Distinctive but usable:** dark ink surfaces, restrained luminous blue accents, fine structural dividers, generous whitespace, crisp data typography. Avoid dashboard templates full of glowing boxes, parallax, excessive gradients and constantly moving lines.
3. **Spatial continuity:** a selected task/context feels continuous when opening a detail panel or switching from plan to execution/result. Animations reveal *cause and effect*, not decoration.
4. **Trust before drama:** verified/unverified results, uncertain billing, budget approvals, account consent and disconnected SSE states have unmistakable distinct treatments. Never communicate success only by animation or color.
5. **Elegant under limits:** usable at 320 CSS px and 400% zoom, for keyboard/screen reader users, reduced motion, low power, offline mode and browsers without newest CSS features.

## The design foundation: tokens, themes and typography

These are **application UI tokens**, **not a reconstruction of the existing approved logo's unverified exact brand HEX colors**. The approved brand remains **navy / blue / gray, no gold**. Token changes across themes must be made centrally; prohibit one-off component hex constants and arbitrary animation timings.

| Token role | Dark-first reference | Light reference | Usage |
| --- | --- | --- | --- |
| `--rv-bg` | `#0B1424` | `#F5F8FD` | Application canvas |
| `--rv-surface` | `#121F34` | `#FFFFFF` | Main cards and panels |
| `--rv-raised` | `#162740` | `#E9F0FA` | Floating/detail layers |
| `--rv-line` | `#2B3C57` | `#CCD9E9` | Functional borders, connection paths |
| `--rv-text` | `#F0F5FF` | `#162238` | Primary text |
| `--rv-muted` | `#B6C6DD` | `#40536F` | Secondary text; verify exact background pairing |
| `--rv-accent` | `#66A8FF` | `#1856AA` | Selection, navigation, focus and active signals |
| `--rv-positive` | `#73E4D7` | `#12675B` | Verified/healthy only |
| `--rv-danger` | `#FCA4A4` | `#A12736` | Failed/irreversible/denied states |
| `--rv-caution` | Separate semantic warning color, **not brand gold**; select using contrast tests | Separate tested warning color | Budget uncertainty and security warning; always icon/text paired |

Recommended type: **Inter** (variable, screen-readable, licensed OFL) as the UI family, **IBM Plex Mono** for identifiers, code, times and tabular numeric metrics, with full locale-capable system fallbacks. Font files are **external URL assets, never embedded or bundled** in JS/CSS/HTML. Prefer few font subsets/weights, `font-display: swap`, declared fallbacks and metrics to minimize shifts. The exact licensed static URLs and supported glyph subsets are an asset-delivery implementation task, not something to fabricate in docs.

**Icons:** an opt-in, small reviewed set of **Lucide** glyphs exported as *separate external static SVG URL assets*, not inline SVG JSX/icon packages or bundled data URLs; preserve ISC/MIT notices where applicable. Text labels/accessible names remain on actions. The **approved Rolvya logo is separate** and must use its genuine verified source; never rebuild the geometric mark from Lucide shapes. If an icon URL fails, text/functionality still works.

**Density and scale:** 4px spatial base; documented spacing tokens `4, 8, 12, 16, 24, 32, 48, 64`px; compact and comfortable density via global tokens, not ad hoc per-screen CSS. Common corner sizes `8 / 12 / 16`px with deliberate hierarchy, not universal pill buttons. Type scale fluid and restrained (`clamp()` with rem); running body text defaults around 14–16px with user text zoom. Use tabular figures for budgets/timing. Visual separators are functional and lightweight; avoid animated shadows and large-area glass blur.

**Theme:** intentional dark and light themes with system preference default and explicit accessible user switch. Preserve semantic contrasts, `color-scheme`, `forced-colors`, `prefers-contrast` and high-contrast focus outlines. Never use opacity alone to make small explanatory text unreadable. Target WCAG 2.2 AA; measure actual rendered states, including translucent overlays/hover/focus, not just isolated token values.

## UI Element architecture — mandatory build order

Build a **Rolvya UI Elements** catalog first; assemble every route and feature from those **same** definitions. A feature **must not** clone a button/table/dialog or invent its own spacing, overlay, motion, keyboard and color rules. Suggested browser source boundaries (not yet created):

```text
apps/web/src/ui/
  tokens/        colors, surfaces, layout, typography, z-index, focus, state, motion
  primitives/    Button, Icon, Text, Surface, Stack, Grid, Divider, FocusRing
  elements/      Input, Select, Field, Badge, Tooltip, Popover, Dialog,
                 Drawer, Tabs, Toast, Progress, Skeleton, Table, Menu, Switch
  patterns/      PageHeader, FilterBar, DetailInspector, SplitView, Timeline,
                 TaskTree, PlanGraphNode, CostSummary, ApprovalReview, EmptyState
  motion/        transition tokens, view-change coordinator, reduced-motion
  state/         shared project/task/attempt, role, budget, connectivity read models
  testing/       component, accessibility, interaction, visual-regression fixtures
apps/web/src/features/
  projects/ execution/ costs-approvals/ connections/ results-activity/
```

**Component API contract (every Element):** reusable typed props; semantic variants and sizes; disabled/loading/read-only/error/warning/empty/unauthorized states; stable test selectors; focus and keyboard specification; accessibility name/description; motion variants and cleanup; responsive/container behavior; source of business data; explicit permission gating; form/validation integration; RTL support; user preference support. All screens compose the same versions and consume **one theme/state source**.

Element coverage to implement before bespoke screens:

| Layer | Canonical components | Reuse/integration obligation |
| --- | --- | --- |
| Foundation | `Text`, `Heading`, `Surface`, `Grid`, `Stack`, `Inset`, `Separator`, `Icon`, `FocusRing` | Token-only sizes/colors; single spacing and typography rules |
| Inputs/actions | `Button`, `IconButton`, `TextField`, `SearchField`, `Select`, `Checkbox`, `RadioGroup`, `Switch`, `FieldError` | Labels and keyboard behavior; no unauthorized mutation from optimistic UX |
| Overlays/nav | `Menu`, `Tooltip`, `Popover`, `Dialog`, `Drawer`, `Tabs`, `Navigation`, `Breadcrumbs`, `CommandPalette` | One focus trap/restoration strategy, safe escape/dismiss, deep-link navigation, overlays kept inside viewport |
| Information | `StatusBadge`, `DataTable`, `RecordList`, `Metric`, `Timeline`, `Progress`, `Skeleton`, `EmptyState`, `ErrorState`, `ConnectionBanner`, `Toast` | Same authoritative status semantics; empty/loading/error/disconnected never conflated |
| Orchestration | `TaskTreeRow`, `GraphTaskNode`, `GraphLegend`, `TaskInspector`, `AttemptTimeline`, `BudgetMeter`, `CostBreakdown`, `ApprovalPreview`, `CredentialModeIndicator`, `ResultEvidence` | Reuse core cards/badges/filter bars; DAG visualization is read-only and distinct from task hierarchy |

Create an isolated interactive **UI Elements gallery/test route** behind development-only build controls, with one example of every visual/semantic state. This is **not** an extra production JS/CSS bundle or new user-facing navigation section. Visual regression and keyboard tests run against the same elements the five real pages import.

## Five sections — one continuous workflow

Exactly the **approved five** top-level navigation sections; do not add a separate "Dashboard" as a sixth section.

1. **Projects** — project summary and fixed plan versions; visual relationship between hierarchical task tree and **read-only** DAG canvas. Selecting a node opens the shared `TaskInspector`; verified graph edges use consistent connection geometry.
2. **Execution** — same project selection and task ID context; stage and attempt timelines, remote worker status, logs and blockers. An active node opens corresponding attempt in `TaskInspector`, preserving provenance of the immutable plan.
3. **Costs & Approvals** — same selected project; four distinct cost classes and optimistic/expected/pessimistic forecasts, reserved/settled/unknown. `ApprovalPreview` shows exact ceiling change, actor and proof, with server-authorized submit; a motion effect may **never** stand in for authorization.
4. **Connections** — same connection cards and `CredentialModeIndicator` for provider/account-specific CLI versus API modes; unsupported entitlement visibly blocked, never silently substituted; secrets never displayed in canvas/logs.
5. **Results & Activity** — same task, attempt and artifact identities link verified results, Git ref, cost and audit event; timelines and filters share the standard pattern.

All sections share one `AppShell`, `ProjectContext`, `TimeRange`, `StatusVocabulary`, `Empty/Error/Loading` patterns and deep-link rules. A selection or filter persists *where legitimate* through navigation and resets visibly when the user loses permission; never carry privileged stale context into another project.

## Motion system — cohesive, richly polished, never uncontrolled

Motion is a **system-owned primitive**: `MotionTokens`, `EnterExit`, `SharedContextTransition`, `FocusMotion`, `StatePulse`, `ListReorder`, `OverlayMotion`. Feature teams may **not** invent one-off durations/easings or import competing animation engines. Use CSS-native transitions/keyframes and supported browser View Transition APIs first; use JS only for coordination that CSS cannot express. No external animation library by default; measure need first.

| Role | Initial duration token (design target, tune by testing) | Use |
| --- | --- | --- |
| `instant` | 0–80 ms | Selection/critical acknowledgment; important state is visible even with motion off |
| `micro` | 90–140 ms | Hover/focus/pressed state feedback |
| `small` | 140–220 ms | Buttons, badges, menus, field validation |
| `medium` | 200–300 ms | Popover/dialog/drawer enter/exit |
| `spatial` | 280–420 ms | Task inspector, route change, node-to-detail continuity |
| `ambient` | No constant global looping | Only bounded, semantic live-progress cue, paused/reduced offscreen |

Use consistent, tested ease-out/ease-in-out curves and 1–2 optional small staggering steps only where the causal relationship improves comprehension. **Animate `transform` and `opacity` preferentially**, not large-scale width/height/top/left/blur/shadow. Avoid infinite node waves, automatic scrolling, confetti and distracting decorative parallax. A task completion transition can propagate a **brief** signal through *validated* outgoing edges only; it cannot claim the dependent task is eligible before Rust says so.

**Navigation:** shared-element transitions should preserve orientation between an item/graph node and inspector. Retain focus, scroll position and accessible label; cancel or replace outdated transitions when SSE state changes or a new route starts. When native same-document View Transitions are supported use them selectively and assign **unique** names to visible matched elements. Cross-document `@view-transition` is **progressive enhancement only**; some widely used browsers lack full support. Unsupported browsers use nonanimated/state-preserving navigation.

**Operational live motion:** foreground only when meaningful. Unverified, paused, offline and cost-uncertain badges should not pulse as if healthy; data may age while SSE disconnected. Prefer static visible status labels plus unobtrusive transition when an authenticated, versioned state actually changes.

**Accessibility:** `prefers-reduced-motion: reduce` and an optional in-app reduced-motion toggle must suppress non-essential spatial motion, replacing it with instant/short opacity feedback; honor system preference by default. Support forced-colors, contrast and battery-constrained modes. No flashing, scroll hijacking or keyboard focus moving with graphical animation.

## Modern CSS, progressive enhancement and hard fallbacks

**Use current standards, not novelty for its own sake.** Native CSS features should produce less JS and more predictable render paths; every enhancement has a functional fallback and browser-support check in the locked test matrix.

- **Core foundation:** modern CSS nesting, custom properties, `@layer`, logical properties (`inset-inline`, `padding-block`), Grid/Flex, `subgrid`, `minmax()`, `clamp()`, `min()`, `max()`, fluid typography, `:focus-visible`, `color-scheme`, `@media (prefers-*)`, `env(safe-area-inset-*)`, `100dvh/100svh` with older `vh` fallback.
- **Component-scale responsiveness:** `container-type: inline-size` + named `@container` queries, so Element layout responds to its **actual panel width**, not merely the browser width. `@supports` guards behavior that needs it.
- **Perceptual color and state:** `color-mix()`, OKLCH (or reviewed compatible color-space), optional `light-dark()` with sRGB fallbacks; no hue-only semantic communication. `:has()` limited to simple, scoped selectors, avoid global subtree-heavy queries.
- **Progressively enhanced overlays and reveals:** `@starting-style` + `transition-behavior: allow-discrete` for modal/popover entry/exit where supported; native HTML `dialog` / Popover API when semantics and focus behaviors pass tests, with accessible conventional fallbacks.
- **Positioning, sizing and motion additions:** CSS anchor positioning and `position-try` guarded by `@supports`, fallback bounded overlay placement; `interpolate-size: allow-keywords` on isolated noncritical disclosures only with fallback; optional `@property` for narrowly justified typed animation tokens; `@scope` where it reduces specificity and is verified.
- **Scroll-driven animation:** optional `animation-timeline` / `view-timeline` for *decorative*, noncritical reading progress only; core control, status, cost and task transitions never depend on it.
- **View transitions:** feature-detect `document.startViewTransition` and guard pseudo-element styles; do not force cross-document transitions or leak sensitive states into snapshots.
- **Containment:** measured use of `content-visibility: auto` and `contain-intrinsic-size` on large *noncritical* offscreen lists; never sacrifice focus, find-in-page, virtualized scrolling correctness, assistive technology or live status announcements.

All experimental/limited CSS must have safe fallback **before** enhancement; avoid deploying a feature merely because one Chromium build supports it. Audit actual 2026 Chrome, Firefox, Safari, iOS Safari and Android Chromium behavior with the exact build and polyfill-free target.

## Responsive and input contract

**No separately designed mobile app.** The five product views and UI Elements adapt through the shared layout rules, including touch, mouse, stylus, keyboard, screen reader, RTL and browser zoom.

- **Small phone (320–479 CSS px):** one-column canvas, top compact header; five top-level destinations exposed via accessible navigation drawer, not squeezed into tiny unlabeled tabs. Sticky context actions only when space permits, safe-area insets, full-height detail sheets, simple task hierarchy first. DAG available in explicit panning view with a complete accessible text tree/list alternative. Data tables become labeled record cards, retaining sorting/filter actions.
- **Large phone and foldable (480–767):** single or short split presentation based on **container width**, navigation remains manageable, dialogs become sheets as needed; handle narrow fold segment and orientation.
- **Tablet (768–1023):** collapsible icon/text rail with labels available on focus or expansion; inspector can be an overlay or right-side sheet. Never hide critical confirmation text behind tooltip-only affordances.
- **Desktop (1024+):** persistent resizable sidebar, central canvas/list, optional right task inspector. Favor task-relevant detail, not three permanently competing scroll containers.
- **Ultrawide (1440+):** max readable text width and intentional negative space; allow DAG canvas to use more width without stretched body copy or unbounded lines. Tune density independently of viewport.
- **All:** browser zoom to 200% and 400%, long translated labels, system font fallback, `lang`/`dir`, RTL logical properties, portrait/landscape, virtual keyboards, safe areas. No page-wide horizontal overflow (local graph pan/scroll is allowed and labeled). Targets should generally be **44×44 CSS px** for touch, never below WCAG 2.2 minimum except documented legitimate exemptions.

Avoid blanket `overflow: hidden` on app root, device-width sniffing, heavy scroll observers and hover-only actions. Use `ResizeObserver` only at controlled graph/layout boundaries when container CSS queries are insufficient, not per row by default.

## Performance engineering is part of design, not an afterthought

1. **The approved artifact limit is binding:** exactly **one** minified app JS containing compiled CSS (plus separate `sw.js`); no emitted standalone app CSS, app/vendor chunks or embedded icon/font payloads. Static external approved brand icons, external static Lucide icon URLs and font URLs allowed by ADR 0043. Component modules may remain independent in source. Strict CSP-compliant CSS mounting and real browser testing are required. No CSS rule/license addition bypasses this.
2. **Bundle discipline:** static import only selected UI Elements/icons at build time; no unneeded animation runtime, charting framework, icon package or heavy editor; tree-shake and audit third-party code. Single-bundle means route-level code splitting is unavailable, so aggressively avoid bulky unused features and measure download/parse/start cost. Compression/cache hash/rollback configured and verified, not claimed from a Vite flag.
3. **State/rendering:** keyed stable task/attempt IDs, fine-grained data selectors, memoization only after profiling, virtualize lengthy tables/audit feeds, level-of-detail/edge culling for large React Flow graphs. Main graph remains a **read-only** projection of the immutable plan. Do not rerender the full graph for every SSE message; batch/coalesce versioned event updates and preserve focus.
4. **Animation:** composite-friendly `transform/opacity`, limit animated area and simultaneously transitioning nodes; no permanent `will-change`, frequent expensive filters, universal box-shadow tweening or uncontrolled backdrop filters. Pause offscreen decorative effects. Test slow devices and low-power use.
5. **Data/network:** paginate/slice server-side via Rust, request cancellation, backpressure and SSE reconnection by event/version cursor; stale data labeled stale. Lazy-load **data**, not JS code chunks, when entering sections. No private credential/cost cache via Service Worker; limited static shell only per security policy.
6. **Core Web Vitals targets (not measured pass claims):** representative p75 good thresholds **LCP ≤2.5s, INP ≤200ms, CLS ≤0.1** where applicable. Additionally profile initial JS transfer, parse/execution time, peak heap, sustained event processing, graph pan/zoom, long tasks and animation frame drops under throttling. Set a **measured** bundle-size warning and performance budget once the first real locked build exists, rather than inventing guarantees for React Flow and one-JS packaging.
7. **Reproducibility:** benchmark reference runs on both smaller mobile devices and ordinary desktops, including slow network, cold cache, 200/1,000/5,000-node graph fixtures, long audit streams, 400% zoom, light/dark, RTL, and reduced motion. Define acceptable rendering thresholds from evidence; automated regression budgets in CI.

## Integrated acceptance gates — do not call it complete before proof

| ID | Scenario and measurable evidence |
| --- | --- |
| UI-001 | Five screens share the **same versioned** token and element implementations; no duplicate route-specific buttons, status colors, dialog logic or easing constants |
| UI-002 | Every Element has tested primary, pending, disabled, error, read-only and relevant permission/empty states; accessible names and labels |
| UI-003 | Task IDs, statuses, budget and result evidence remain consistent across Projects → Execution → Costs → Results; stale SSE does not invent a success |
| UI-004 | Full/reduced motion, system preference and manual override; animation cancellations, focus restoration and no content loss across navigation |
| UI-005 | Motion style budget: responsive under load, no unnecessary layout animations or infinite background loops; reduced motion path is equivalent |
| UI-006 | 320/390/768/1024/1440/2560px widths, landscape, 200/400% zoom, RTL, safe areas; no page-level horizontal overflow |
| UI-007 | WCAG 2.2 AA testing: keyboard order, visible/unobscured focus, names, colors, tooltips, dialogs, menus, target sizes, screen readers |
| UI-008 | Graph accessible tree/list alternative; keyboard selection, graph panning/zoom control and node inspector without drag dependency |
| UI-009 | Cost uncertainty and explicit budget ceiling actions present **unambiguous** confirmation/denial; backend remains sole authority |
| UI-010 | Production asset inspector: one external app JS, CSS inside it, standalone `sw.js`, external-only font and icon URLs, real CSP compatibility |
| UI-011 | Native new CSS features tested per target browser with non-breaking `@supports`/static fallbacks; no critical task hidden by unsupported CSS |
| UI-012 | Core Web Vitals, transfer/parse, large-graph, long-feed and animation/perceived latency **measured** on reproducible fixtures |
| UI-013 | User-readable unavailable/offline/unauthorized/expired-connection states; animations never conceal error or create fake success |
| UI-014 | Original approved Rolvya logo remains unchanged and its actual asset provenance is checked before release; no gold brand theme |
| UI-015 | External font/icon OFL/ISC (and applicable Feather MIT) notices, security headers, theme continuity and visual regression baselines verified |

Map these to existing [product acceptance criteria](acceptance-criteria.md), especially AC-153–164 for browser artifacts; they **augment** existing tests rather than renumbering their approved IDs.

## Source order and review policy

1. **U0 — tokens and UI Elements API**: theme/light/dark, surfaces, focus/role tokens, typographic/icon asset contract and interactive Elements gallery.
2. **U1 — unified interaction/motion foundation**: overlay semantics, shared transitions, reduced-motion and accessible feedback. Motion is designed with each Element, not patched on after page design.
3. **U2 — reusable orchestration patterns**: graph/task tree, inspector, cost/approval, connection, evidence panels; all consume identical shared elements.
4. **U3 — compose five sections** from patterns, preserve project/task cross-screen context and responsive variants; test each component at several container sizes.
5. **U4 — native CSS enhancement & optimization**: profile and add only supported, measurable CSS improvements; prove strict single-JS/CSS build and fallback behavior.
6. **U5 — device/accessibility/regression certification**: cross-browser interaction tests, visual review, reduced motion, real server-side security gating, response/load measurement.

The Owner delegated **creative UI direction** and requested it be documented, **not** implementation of frontend source or paid infrastructure in this turn. Build stages above apply when actual frontend implementation is scheduled. No new GitHub Issue is required.

## Sources (technical verification, not production test evidence)

- [MDN: View Transition API](https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API)
- [MDN: `@starting-style`](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/%40starting-style)
- [MDN: CSS container queries](https://developer.mozilla.org/en-US/docs/Web/CSS/Guides/Containment/Container_queries)
- [MDN: CSS anchor positioning](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/position-anchor)
- [MDN: `interpolate-size` browser limitations](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Properties/interpolate-size)
- [MDN: View transitions browser support](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/%40view-transition)
- [MDN: CSS performance and content visibility](https://developer.mozilla.org/en-US/docs/Learn_web_development/Extensions/Performance/CSS)
- [MDN: reduced motion](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/%40media/prefers-reduced-motion)
- [W3C: WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [Inter licensed font](https://github.com/rsms/inter)
- [IBM Plex Mono licensed font](https://github.com/IBM/plex)
- [Lucide icon license](https://github.com/lucide-icons/lucide/blob/main/LICENSE)
- [web.dev: Baseline browser support](https://web.dev/baseline)

## Traceability

[ADR 0042](../decisions/0042-five-section-web-panel-navigation.md) · [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) · [UX accessibility contract](web-ux-and-accessibility-contract.md) · [browser platform contract](../architecture/web-platform-artifact-and-capability-contract.md) · [frontend source/build stages F0–F4](../development/web-frontend-source-and-build-plan.md) · [approved brand](brand-positioning.md) · [project status](../project/status.md).
