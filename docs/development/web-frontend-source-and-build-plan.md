---
title: Initial Frontend Source Layout and Verified Single-Bundle Build Plan
status: Proposed implementation plan within approved ADR 0043 constraints
category: development
last-reviewed: 2026-10-25
tags: [frontend, source-layout, build, vite, ci, pwa, csp, ux]
---

# Initial frontend source layout and verified single-bundle build plan

## Purpose and decision boundary

The Owner approved the **core frontend stack and packaging invariants** in [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), and previously approved five top-level panel sections in [ADR 0042](../decisions/0042-five-section-web-panel-navigation.md). This document is a practical **source/module and implementation sequence proposal** to reach that already approved target. Detailed UI visual design, font and icon selection, exact runtime routes, visual components and precise provider accounts **are not being chosen here**.

**Current actual repository state (checked 2026-10-25):** documentation-only; no `apps/web`, `Cargo.toml`, React source, Vite build output, GitHub Actions workflow or executable integration is verified. The paths and examples below are **planned**, not existing files.

## Proposed repository location and boundaries

Create one browser frontend project under `apps/web/`. Keep Rust-only backend under its separately proposed `apps/controller/` workspace; do not place a Node/TypeScript backend in production.

```text
rolvya/
  apps/
    web/                                # proposed browser-only frontend
      package.json
      package-lock.json                 # lock runtime/build dependency tree (npm proposed)
      tsconfig.json
      vite.config.ts
      index.html                        # static template (or nonce-aware Rust-served HTML)
      src/
        main.tsx                        # single browser application entry
        app/
          App.tsx                       # navigation shell, no approved look-and-feel yet
          routes.ts                     # route declarations / contracts, package choice open
        api/
          client.ts                     # authenticated REST + errors/versions
          events.ts                     # scoped SSE, reconnect + stale-state recovery
        auth/
          session.ts                    # browser session display/reauth flow (not authority)
        features/
          projects/
          execution/
          costs-approvals/
          connections/
          results-activity/
        graph/
          dag-view.tsx                  # read-only React Flow graph
          hierarchy-view.tsx            # separate accessible task/group tree
          models.ts                     # typed immutable plan vs mutable attempt overlay
        accessibility/
          focus.ts                      # focus and status semantics, no visual decisions
        i18n/
          locale.ts                     # Intl/RTL/fallback contracts; locales undecided
        platform/
          register-sw.ts                # separate sw.js registration/update handling
          capabilities.ts               # progressive feature detection
        styles/
          entry.css                     # source CSS, not an emitted production CSS file
          mount.ts                      # ?inline style strings + CSP-aware mounting
      public/
        sw.js                           # separate standalone Service Worker
        manifest.webmanifest            # create when real name/icons/scopes specified
        icons/                           # no invented/placeholder brand/icon assets
        locales/                         # optional .json only, no extra executable JS
      scripts/
        verify-dist.mjs                 # CI-only artifact count / policy gates
        check-browser-assets.mjs        # CI-only manifest/SW/file references
      tests/
        build/
        browser/
  docs/
    development/web-frontend-source-and-build-plan.md
```

This is a **logical structure**, not a requirement to create every empty folder immediately. Source CSS and React Flow CSS may be split into multiple **source files**, but production CSS must exist **only within the single external application JS bundle**. Browser assets and locale data are **not additional application bundles**. JS in `scripts/` runs **at build/test time only**, never as a production server.

## Concrete initial build approach to validate (not yet implemented)

**Use Vite's supported `?inline` CSS query for explicit source imports** so it returns processed CSS text instead of emitting/injecting a normal stylesheet. Import the React Flow stylesheet the same way. Build a **single normal HTML-entry browser app**, not a library-mode build or a full HTML-inlining plugin.

Example **illustrative TypeScript pattern** for a build prototype (not a complete or tested application):

```ts
// src/styles/mount.ts — illustrative only
import baseCss from './entry.css?inline';
import reactFlowCss from '@xyflow/react/dist/style.css?inline';

export function mountStyles(styleNonce: string): void {
  const style = document.createElement('style');
  style.setAttribute('nonce', styleNonce);
  style.textContent = reactFlowCss + '\n' + baseCss;
  document.head.appendChild(style);
}
```

The Rust HTML-serving layer must supply an appropriate **unique per-response cryptographic CSP nonce** to authorized first-party JS and matching response policy, **if nonce-based loading is selected**. A reproducible build-derived exact CSS hash may be evaluated as a static-host alternative. Neither method is presumed working until tested with React Flow in Chrome, Firefox and Safari. Do **not** use global `style-src 'unsafe-inline'` or `script-src 'unsafe-eval'` as an unreviewed shortcut. React Flow can use inline SVG **for graph edges**: this is distinct from prohibited **embedded icon glyphs**. Verify dynamic graph node positioning/DOM styles against the chosen CSP in browsers.

**Critical:** Vite `build.cssCodeSplit: false` **still emits one CSS file**; it does not meet the requirement. Keep application CSS imports as `?inline`, implement a verified minification step for CSS string output when needed, and reject all extracted emitted `.css` in CI.

### Vite/Rolldown constraints (version-lock implementation required)

- Use a **single HTML entry**, with an **external** `app.[contenthash].js` reference; no JS/CSS inlining into `index.html`.
- Current Vite/Rolldown production options are `build.rolldownOptions.output.codeSplitting: false` to inline dynamically imported modules. Prefer this over **deprecated** `inlineDynamicImports`. Test order-sensitive side effects after disabling splitting.
- `build.assetsInlineLimit: 0` helps stop accidental base64 asset inlining. Forbid font/icon importing or transforming to inline SVG/data URIs regardless of asset-size thresholds.
- Enable JS and CSS minification with the **exact locked toolchain**, and explicitly verify the CSS text output; do not assume Vite minifies `?inline` content identically to emitted stylesheets. Production sourcemaps must not be public by default.
- Reject second entry, chunk, runtime polyfill file, `.mjs`, unexpected JS worker/worklet or separate `.css`. Only `sw.js` is an approved standalone JS exception under ADR 0043.
- Always evaluate package upgrades against **artifact inventory and browser smoke tests**, not only TypeScript compilation. Root lockfile/workspace and dependency version choices will be finalized when scaffolding is implemented.

### Browser asset and PWA bootstrap

- `public/sw.js` remains **standalone and same-origin**, with controlled versioning, update behavior and narrow static shell cache. The Service Worker is **not** included inside `app.[hash].js`, and must not import additional executable JS chunks.
- `index.html`, web manifest, external favicon/touch/PWA icons, correct MIME, secure HTTPS/CSP/cache headers are validated against the actual deployment base URL. Real logo/icon assets and manifest design color values **are blocked on approved visual assets**; no invented placeholder should appear as finished branding.
- Optional translation catalogs are allowed as external `*.json` data, not as extra JS. Load with versioned/bounded requests, preserve `lang`/`dir`, Intl formatting, RTL and fallback behavior.
- No offline service worker job queue or automatic replay of budget increases, approvals, provider launches, user credential changes or other protected actions. A cached offline shell must never imply current project billing/worker status.
- Font and icon assets remain **external URLs**, with later provider choice and fallback. An external provider's own stylesheet (e.g., optional Google Fonts) is not part of the local `dist` CSS inventory, but its privacy, CSP, offline, latency and "no extra CSS network request" implications must be explicitly reviewed before selecting it. Direct separate font files plus bundled `@font-face` CSS are also possible. **No font or icon vendor chosen now.**

## Automated production artifact gate — fail CI on mismatch

The planned `verify-dist.mjs` must scan the **actual emitted** `apps/web/dist/` recursively and fail with useful filenames for:

1. **Not exactly one application `.js`** (e.g., `app.abc123.js`) or missing/multiple `sw.js`; other runtime `.js` or `.mjs` files are rejected. Build/dev scripts outside `dist/` are irrelevant.
2. Any emitted `.css` file, application JS module-import references to other chunks, inline executable app JS in HTML, or accidental duplicate script entries.
3. Bundled WOFF/WOFF2/TTF/OTF/EOT, font icon payload, embedded icon SVG glyph/component/data URI, unintended base64 fonts/icons, unapproved inlined assets or undesired source maps in a publicly deployable directory. **Legitimate React Flow edge SVG markup is allowed; icon payload checks must be narrowly scoped and not forbid all SVG graph rendering.**
4. Broken `index.html` links to the hashed JS, missing `sw.js`/manifest expected assets, invalid MIME expectations or references to unshipped icons. Manifest/icon validation can only pass once **genuine approved assets exist**.
5. Bundle/CSS no-op: build emits one JS but stylesheet not applied or React Flow graph broken. Prove via browser smoke test, including CSP violations, no-JS/blocked-font fallback and cache updates.
6. Missing budget/security/failure UX verification: authentication, stale SSE/reconnect, accessible DAG/list alternative, safe service-worker offline behavior, translated copy and browser feature denial.
7. Document test commands, browser/OS versions, bundle raw/Gzip/Brotli bytes, measured startup/render/parse cost, toolchain and known limitations. Do not assume that a single bundle necessarily performs better than code splitting.

Build tools and CI may use multiple internal/source JS files; **only final production browser app output** is subject to the single-app-JS restriction.

## Suggested incremental implementation gates

| Gate | Scope | Acceptance / evidence | Dependencies |
| --- | --- | --- | --- |
| **F0 — Minimal source scaffold** | `apps/web` TypeScript/Vite+React, single app entry, neutral test content; lock actual versions | `typecheck` and dev build, no UI design selected | Approved ADRs 0042/0043 |
| **F1 — Single-JS build proof** | `?inline` for own/React Flow CSS, CSS mount and CSP strategy, Rolldown one output, `verify-dist.mjs` | Exactly one minified app JS and separate `sw.js`, zero emitted CSS/other JS; real browser shows CSS under CSP | F0 |
| **F2 — Read-only navigation/mock data** | Five approved sections, hierarchy versus DAG, visible error/blocked/verification state, a11y/Intl scaffolding | Accessible graph alternative and no mutation of frozen DAG in deterministic fake-state tests | F1 |
| **F3 — Rust integration** | Versioned REST/SSE client, secure session/role/CSRF handling, cost/approval read models, redacted events | Auth/permissions and stale-event/refresh tests against Rust fake handlers; no paid provider launch | F2 + backend contracts |
| **F4 — Browser/PWA certification** | Manifest/icons after brand assets approved, SW/cache lifecycle, i18n and RTL, offline rules, browser matrix | AC-153–164 with actual artifacts and browser evidence; no sensitive cache or unapproved browser permissions | F1–F3 + real visual assets |

**Recommended immediate next engineering slice:** implement **F0–F1 in a limited branch/PR**, with neutral placeholder *text* only and no fabricated logo/icon/fonts or visual UI design. This is **not an Owner approval to bypass a later review of actual code/security results**. No AWS account, API key, CLI session, paid worker or remote Git integration is required to prove the browser packaging contract.

## User-facing vs implementation decisions

Already settled: five panel sections, React/TypeScript/Vite, React Flow, Rust Axum REST+SSE, immutable DAG server authority, one application JS containing CSS, separate `sw.js`, no bundled fonts/icons, UX/accessibility/browser completeness requirements.

Left to implementation or Owner design review: concrete folder presence, package version/lock manager, exact static hosting/nonces and CSP hashes, CSS minifier operation, manifest icon assets and name, selected font/icon provider, graphic/color/UI components/layout, routes/schema, shipped locales, dependency graph layout algorithm, browser version support floor, and first-plan-activation UX. Deferring UI aesthetics does not defer **accessible interactions and security**.

## References / traceability

- [ADR 0042 — approved panel sections](../decisions/0042-five-section-web-panel-navigation.md)
- [ADR 0043 — single-bundle and browser constraints](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md)
- [Browser output and feature contract](../architecture/web-platform-artifact-and-capability-contract.md)
- [Browser UX/accessibility contract](../product/web-ux-and-accessibility-contract.md)
- [AC-153–164 verification matrix](../product/acceptance-criteria.md)
- [Vite CSS `?inline`](https://vite.dev/guide/features)
- [Vite current build options](https://vite.dev/config/build-options.html)
- [Rolldown `codeSplitting: false`](https://rolldown.rs/reference/OutputOptions.codeSplitting)
- [React Flow stylesheet requirement](https://reactflow.dev/learn/troubleshooting/common-errors)
- [MDN CSP style nonces and hashes](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/style-src)
