# Connected Control panel implementation

G1-07 implements `apps/web/` with React, strict TypeScript, Vite/Rolldown and read-only React Flow. It preserves ADRs 0042/0043/0048 and the [Connected Control design system](../product/connected-control-design-system.md). This is a browser foundation, not production activation or MVP release certification.

## Run and build

Use Node 24 and the committed npm lockfile:

```sh
cd apps/web
npm ci --ignore-scripts
npm run dev
```

`dev` explicitly starts the synthetic demo. `dev:connected` uses only the controller client. `build` emits the connected production application; `build:demo` emits a separate, visibly labeled `dist-demo/`. Production cannot select fixture data or the Elements gallery through URLs, local storage or API failure. Demo source is eliminated from its executable bundle. There is no production Node service.

```sh
npm run lint
npm run typecheck
npm run test:ci
npm run build
npm run verify:dist
npm run check:browser-assets
npm run build:demo
node scripts/verify-dist.mjs dist-demo
npm run licenses
npm run test:browser
npm run measure
```

Browser tests use optimized builds, not Vite HMR. `ROLVYA_CHROMIUM` can identify an installed Chromium executable. CI installs pinned Playwright Chromium/Firefox/WebKit and selects them through `ROLVYA_BROWSER_ENGINES`. Missing or blocked browser installations are not passing tests.

## Implemented product

The five sections share tokens, controls, overlays, status badges, tables and orchestration patterns. Project/task hash routes preserve selection across navigation and browser history. Projects has separate hierarchy and dependency views with one task inspector; groups never execute. Execution separates termination, pending verification, acceptance and unknown provider state. Costs distinguish holds, settlement, provisional and unknown liabilities; integer minor units use exact BigInt/Intl formatting. Approval previews cannot submit or mutate ceilings. Connections distinguish API/CLI accounts and current blockers. Results distinguish accepted evidence, candidates and isolated publication.

The demo uses synthetic A/B/G/C data. Advancing it simulates receipt of independent evidence and group acceptance without creating DAG nodes, changing budgets or contacting a backend. It never represents actual account entitlement, cloud execution, email, model output or repository publication.

Dark/light themes, English locale variants, demo-only expanded/RTL QA labels, external Inter/IBM Plex Mono/Lucide resources and system-font/labeled-control fallbacks share one implementation. Native dialogs trap and restore focus. The shared select-only combobox keeps focus and supports arrows, boundaries, typeahead, touch and nested dismissal without the native select creation path that emitted strict-CSP violations in WebKit; no policy permission is relaxed. Motion uses common CSS tokens and optional native View Transitions, with reduced-motion fallbacks. Phone layouts start with the hierarchy and use drawers. Browser reflow simulations do not establish physical-device or assistive-technology certification.

## Rust and browser authority

`human_auth::sessions::owner::panel` reads bounded complete plan/task projections in the existing session/membership transaction. It rechecks current browser evidence, session/security state and binding before commit; reads do not extend human activity. It omits instructions, context bytes, credential references, legacy Owner strings and unsupported metadata. A runtime success without independent acceptance is displayed as unknown. Project listing is scoped and paginated (1–100). Plans over 5,000 tasks, 64 KiB per definition or 1 MB combined definitions are refused before materialization; response bytes are also bounded.

The existing production `BrowserAuthority` is unavailable. Therefore `browser_panel_routes()` deliberately returns `503`, `no-store`, redacted JSON for REST and SSE. No request can install a success verifier. The frontend has a tested contract for these routes and the exact Rust-serialized projection, but **real authenticated browser delivery and real-time SSE are not activated**. Existing Owner-only budget library policy is preserved; financial, connection, attempt, result and activity read projections remain unavailable in this narrow adapter. Their full UI is demonstrated only with explicitly synthetic data.

The client rejects incomplete/cyclic/overdeep plans, cross-project connection scopes, contradictory acceptance/publication metadata, broken task/attempt attribution and capability inconsistencies. SSE carries versioned invalidations only: duplicates are ignored, gaps refresh snapshots, refresh bursts coalesce to at most two per second, reads have a 10-second deadline, outages mark data stale, and explicit 401/403 clears private state and stops reconnection. No URL bearer or worker-to-human authentication conversion exists. Clear-local-view is explicitly not server logout. Trusted browser login/logout, CSRF-protected commands and reviewed SSE transport remain follow-up work.

## Production artifacts and CSP

The audited output has exactly four files: `index.html`, one minified content-hashed `app.*.js`, standalone `sw.js`, and `manifest.webmanifest`. Compiled CSS is a string in that single JS, mounted with an exact SHA-256 CSP authorization. HTML carries script SRI. No vendor/lazy chunk, stylesheet, font binary, bundled icon library, source map, `unsafe-inline` or `unsafe-eval` is permitted. The auditor examines actual recursive output, parsed HTML, JavaScript AST, CSS commitment, CSP, manifest and references; negative tests reject extra executables/assets, unusual script endings and broken integrity. HTML parsing executes no scripts and loads no resources.

React Flow vendor CSS is placed in the first cascade layer. Its positioning uses browser CSSOM; genuine browser tests check the strict policy without widening it for inline attributes. Graph SVG geometry is distinct from functional icon payloads; functional icons are external images with text fallbacks.

Vite preview applies tested CSP, no-referrer, nosniff, frame restrictions and an explicit public-static marker. A future Rust/static deployment must serve matching headers and static-only marked responses. The production hosting/security-header integration itself is not activated by a successful local preview.

## PWA and recovery

Only the four exact public static shell paths can enter the versioned cache. All cache fetches omit credentials. APIs, queries, foreign origins, authenticated bearer requests and mutations are excluded. No private snapshots, source, billing records, challenges or offline command queue are stored. A worker preserves its coherent HTML/JS generation until a new shell is ready; explicit activation requires matching message and client origins, then cleans only older Rolvya caches. Initial installation does not reload the page. Offline/reconnection state never implies task failure or settled charges.

The existing logo sheet does not establish a canonical release icon/master. It is unchanged. The application uses the Rolvya text name, and the manifest has no fabricated icons. Favicon/PWA asset approval and installability certification remain release gates.

## Evidence and limits

Tests cover shared controls/focus, graph relationships, pagination shrink, exact currency exponents, response validation, REST/SSE revocation/replay/gaps, CSP, all five sections in both themes, six viewport widths, RTL, touch/reflow/forced colors, external-resource failure, static offline behavior and private-cache exclusion. Two independent source reviews produced reproduced regressions for graph containment, stale pagination, tab semantics, display consistency, pre-allocation byte limits, known SSE revocation and coherent shell updates. Financial over-ceiling display and vendor-layer contrast were also corrected.

`measure` profiles 200/1,000/5,000-task synthetic fixtures. The graph intentionally previews at most 200 nodes; the complete list remains searchable/paginated. Reports separate source count from rendered count, raw/gzip/Brotli size, Node V8 parse/compile, Chromium trace events, navigation/render time, heap, layout shifts and frame samples. The separate demo-only stress harness also measures 1,000 and 5,000 simultaneously rendered nodes; production still limits the preview to 200. This is not real SSE throughput or physical-device performance.

Frontend dependency auditing found a development-only minimatch advisory; the exact patched 11.0.1 override is tested with the locked ESLint/TypeScript toolchain. No advisory suppression is added. Dependabot #1 remains inaccessible/unidentified. The separate unpatched locked RSA RustSec advisory remains visible and must be distinguished from the active Linux production graph. Exact test counts, dependency database revision and final-head GitHub outcomes belong in the PR evidence.

Production human browser transport, final icons, real installation/provider/remote isolation gates and physical-device/screen-reader validation remain incomplete. No migrations, financial policies, worker credentials or production activation guards are changed. No real credentials, paid operations or user-repository publication are used.

Rendered optimized-demo evidence: [desktop](images/connected-control-desktop.png) and [phone](images/connected-control-mobile.png). These show genuine Chromium rendering with external-resource fallbacks, not a design mockup or physical-device capture.
