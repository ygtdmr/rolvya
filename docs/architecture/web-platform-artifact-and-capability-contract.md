---
title: Browser Platform, Production Assets and Build Verification Contract
status: Approved architectural constraints; implementation pending
category: architecture
last-reviewed: 2026-10-25
tags: [frontend, single-bundle, build, pwa, service-worker, i18n, browser-compatibility, assets, security]
---

# Browser platform, production assets and build verification

## Status and scope

[ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) **approves the output and browser-completeness constraints**, but no browser app, manifest/icons, Service Worker or actual tested build exists yet. This document defines the **deliverable and verification catalog**, not any visual asset choice or authorization to activate every browser capability.

**Terminology:** 'one JS' = exactly one **application** JS file in production. A browser-managed **`sw.js` is mandatory and separate**, so the full `dist/` may include **two .js files**, but only one is app code. Source may contain any number of TS/TSX/CSS modules. Fonts/icons are never inlined or compiled into application code.

## Proposed distribution tree — planned file roles, not files already produced

```text
dist/
  index.html                       # semantic entry document and links, not embedded app JS
  app.[content-hash].js            # THE ONE minified application JS, includes compiled CSS
  sw.js                            # separately served Service Worker; not an app JS chunk
  manifest.webmanifest             # separate application manifest JSON
  favicon.ico                      # broad legacy browser fallback icon (asset TBD)
  favicon.svg                      # only if actual accepted icon asset has SVG source
  apple-touch-icon.png             # touch/home-screen icon; image TBD
  icons/
    pwa-192.png                    # 192x192 install icon
    pwa-512.png                    # 512x512 install icon
    pwa-maskable-512.png           # valid maskable-safe-zone variant if required
  locales/                         # optional external JSON catalogs, NOT JS chunks
    <locale>.json
  [other-conditional-platform-assets]
```

**Not in production output:** ordinary `.css` files, `vendor.js`, route chunks, framework/runtime chunks, extra polyfill JS, sourcemaps publicly accessible by default, embedded WOFF/font binary, icon SVG paths/components inline in HTML/JS, inlined CSS/JS in `index.html`, any data-URI/Base64 icons/fonts, unapproved offline sensitive data. Only actual approved branded icon assets may be used—**no placeholder logo, font or icon supplier shall be silently finalized**.

### Browser/OS file and metadata inventory

| Deliverable | Policy | Semantics, testing, conditions |
| --- | --- | --- |
| `index.html` | Mandatory | `doctype`, document `lang` and `dir` adapted to locale, UTF-8, viewport incl. zoom, semantic landmarks, real title/meta description, links to app JS/manifest/favicon/touch icon. Keep informative no-JS/auth/network failure notice without showing private data. |
| Exactly one `app.[hash].js` | Mandatory | Minified compiled application and CSS payload. Hashed immutable-cache version; one HTML reference; no extra script chunks including legacy/polyfill bundles. |
| Separate `sw.js` | Mandatory for approved PWA posture | Secure-origin scoped, updateable SW script, separate JavaScript MIME. No `importScripts` side bundles or user-session sensitive cache. Registers only where browser supports it. |
| `manifest.webmanifest` | Mandatory | Separate valid manifest with stable `id`, `name`, `short_name`, `start_url`, `scope`, supported `display`, `icons`; conditional `description`, `background_color`, `theme_color`, `lang`/localization if supported. No fabricated colors/design values. HTTPS install behavior verified by browser. |
| Favicon `.ico`, optional `.svg`/PNG | Mandatory a usable favicon; formats chosen when assets approved | Serve as external image, correct MIME, test browser/tab/bookmark fallbacks. Do not derive a logo without approved source. |
| PNG PWA icons (at least 192 and 512) | Mandatory for Chromium installability | Image sizes, aspect ratios, transparent/background rules reviewed after UI asset approval; external URLs. Maskable-specific image tested for safe zone where included. |
| `apple-touch-icon.png` | Targeted Apple mobile compatibility | External touch icon size/shape evaluated with iOS Safari, not assumed universal. |
| `locales/*.json` | Conditional | If translation payloads are loaded separately, use **JSON/data** requests only, not extra JS files; alternatively include copy in the one JS. Check fallback, versioning and accessibility. Locale list not yet chosen. |
| `robots.txt` | Depends on exposure | If publicly reachable, keep authenticated admin paths out of indexing; `robots.txt` is **not** an access-control boundary. Use proper auth and `noindex` headers/meta as relevant. |
| `sitemap.xml` | Only if public indexable pages are built | Do not publish protected SPA routes or user-private project URLs. Not needed for private-only dashboard. |
| `/.well-known/security.txt` | Recommended after security contact/process exists | Publish a real valid security reporting contact and expiry, not invented contact data. |
| `browserconfig.xml`, tile art, `safari-pinned-tab.svg`, legacy Apple startup images | Conditional legacy/platform extras | Generate **only** for real supported browser/platform need; don't create assets blindly. |
| `/.well-known/assetlinks.json`, `/.well-known/apple-app-site-association` | Conditional native-app associations | Only when an approved Android/iOS native app or verified universal/app link exists. **No native client currently approved for v0.1.** |
| `opensearch.xml`, web share target, protocol/file handler registrations, shortcuts, screenshots | Conditional UX/browser integration | Manifest members/files enabled only after working and secure feature adoption, not to maximize checkboxes. |
| Offline/static fallbacks, legal/privacy pages | Depends on deployment/legal scope | Offline UI can be inside `index.html`/single app bundle or separate **HTML/data**, never extra app JS. Actual legal text requires owner-approved accurate content. |

The inventory can expand with **genuine** browser/platform needs. A nonexistent or unused artifact must not be claimed as 'required by all browsers'.

## Compilation and bundling verification requirements

1. Vite+TypeScript build for exactly one browser application entry. The selected, **version-locked** Vite/Rolldown/Rollup output configuration must inline supported internal dynamic imports, disable manual JS chunk splitting and eliminate external runtime JS imports. Beware: output `inlineDynamicImports` is single-input-only and can change dynamic-import execution order; verify lazy initialization behavior.
2. Compile/minify CSS from source modules **into JavaScript** and mount one CSP-compliant stylesheet at runtime. Merely setting `cssCodeSplit: false` **produces an external .css file**: add a tested transformation/plugin, or a maintainable custom build step, rather than an unverified config assertion. Do **not** use plugins whose goal is one HTML file by inlining the whole app into `index.html`.
3. Disable asset embedding (`assetsInlineLimit: 0` where appropriate) and prohibit font/icon imports, SVG component transforms for icons, data URI resources and CSS `@font-face` with bundled binary sources. External fonts/icons use separately fetched and CSP-allowlisted URLs only, chosen in the UI phase. Do not force online font availability: test system fallbacks.
4. Service Worker is separately authored/built/deployed without an application chunk dependency. Register it at same origin over HTTPS. Validate coherent old/new app JS/cache/SW upgrades, cache-busting HTML behavior, stale SW recovery and rollbacks. Preserve functional app even when SW unsupported/disabled.
5. One hashed bundle is eligible for long-lived immutable caching; `index.html` and `sw.js` require update-friendly caching. Manifest and external assets have reviewed cache policies. Avoid startup-screen breakage if an external icon/font CDN is blocked.
6. Strict CSP **without blanket `unsafe-inline`**: test whether injected stylesheet uses an authorized content hash or per-response nonce. With static HTML and variable CSS, hash/nonces must match real build/response behavior; do not pretend JavaScript being authorized also authorizes inserted styles. Avoid `unsafe-eval`; keep external providers allowlisted. Verify chosen CSP in browser integration tests.
7. Build assertion operates on final deployable `dist/`, not source file count: exactly one **application** `*.js`, exactly one separate `sw.js`, zero `*.css`, no extra `*.mjs`/JS runtime files, no inline application script/style in HTML, no JS imports requiring missing files, no inlined font/icon/data URI payload, valid manifest and icon references. Reject silently generated chunks/workers/polyfills and publicly shipped source maps.
8. Optimize Gzip/Brotli, target browsers deliberately and measure parse/hydration/render memory and cost of non-code-split JS. Single-bundle is a user-approved tradeoff, **not** an unmeasured performance improvement.

## Browser feature and progressive-enhancement matrix

| Feature | Baseline desired UX | Unsupported/denied/offline behavior |
| --- | --- | --- |
| Installability/PWA and stand-alone window | Correct web app manifest + icons, safe install prompt when applicable | App remains ordinary responsive website; install is not forced |
| Service Worker | Lifecycle update + carefully scoped static shell cache | Normal network mode; show connectivity and retry feedback |
| Offline experience | May display safe cached shell, localized connectivity message, read-only offline notice | **No** offline approval, billable task dispatch, account changes or automatic queued retries |
| i18n/l10n | Language negotiation, explicit user locale selection, `Intl` date/number/currency formatting, pluralization, `lang`/`dir`, timezone and RTL-ready layout | Supported fallback locale shown clearly; no unreviewed machine translations |
| Accessibility | Semantic HTML, keyboard DAG/tree alternatives, visible focus, reduced motion, ARIA live states for important updates, zoom/high contrast | Readable linear/list view when graph interaction unsupported; no mouse-only requirement |
| Responsive input | Mouse, touch, keyboard, screen readers, mobile viewport and browser zoom | Do not gate actions on hover/drag/touch only |
| State and live updates | Authenticated SSE with snapshot and cursor/version reconciliation | Retry with backoff and authenticated REST refresh; 'stale/disconnected' state, not fabricated current data |
| Notifications / Web Push | Opt-in for user-relevant alerts **only if later implemented and authorized** | On-panel alert/inbox is primary fallback; never request permission on page load |
| Clipboard and native share | On explicit user action, secure clipboard/share where available | Selectable text/copy or downloadable approved artifact as suitable |
| File picker/downloads | Deliberate export/import only if product requirement exists; scope/size validation | Basic secure download/file input fallback; never request broad filesystem access |
| Browser history/deep links | Stable, authorized, accessible routes; correct back/forward/focus and refresh | Server SPA fallback or safe 404/401; no private data in URL |
| Storage/cookies | Server-side sessions, minimal nonsensitive client preferences if needed | Do not use localStorage/IndexedDB for long-lived secrets or finance authority; no trust in client storage |
| Media, geolocation, camera, mic, Bluetooth, USB, NFC, etc. | **Not automatically enabled:** evaluate only when Rolvya has an approved use case | Remain unused and blocked by permissions policy; never request ambient privileges |
| Theme/preferences | Respect browser/OS reduced-motion, contrast, locale and color-scheme signals where appropriate | Preserve accessible defaults without choosing final visual palette implementation now |

**Not all Web APIs work across all browsers, versions, operating systems or install contexts.** The correct completeness criterion is *all relevant v0.1 browser integrations accounted for, tested, with documented fallback or explicit non-applicability*. Capability detection and user permissions take precedence over unsupported polyfills that add JS bundles.

## Security and operational browser support (headers/config, not necessarily files)

- HTTPS with valid TLS; HSTS after deployment validation, appropriate CSP (`script-src`, `style-src`, `worker-src`, `font-src`, `img-src`, `connect-src`, `frame-ancestors`), `X-Content-Type-Options: nosniff`, `Referrer-Policy`, `Permissions-Policy`, safe embedding control and scoped CORS when truly needed.
- Cookies: `Secure`, `HttpOnly`, `SameSite` appropriate to real auth flow, reauthentication and CSRF defense for mutations; no secrets/session tokens embedded in front-end JS, URLs, browser notification payloads or Service Worker caches.
- Avoid open redirects, XSS/DOM injection, untrusted HTML, cache poisoning and malicious third-party font/icon execution. Review external provider privacy/network failures; self-hosting an *external asset file* is allowed and is distinct from inlining bytes.
- Web app manifest, `sw.js`, icons and localization files served with correct MIME/cache headers. SW and push never act as an authorization source.
- Accessibility and locale hooks are first-class app requirements even when browsers lack optional APIs.

## Planned cross-browser acceptance evidence

At implementation, choose a documented supported-version matrix and run real or automated browser scenarios for **Chromium-based Chrome/Edge, Firefox, Safari on macOS and iOS Safari, and Android Chromium**, prioritizing current stable supported versions. Validate installability where offered by each browser, keyboard/screen reader, RTL, zoom, cached shell, locale/timezone, no JS/slow JS, platform icon, permissions denied, disabled SW, offline, stale SSE, hard-budget stop and UI approval step-up. Record known deviations; do not promise all OS integration hooks are universally implemented.

## References

- [Vite build options](https://vite.dev/config/build-options.html) — CSS split and asset inlining are not the same as a single JS file.
- [Rollup inline dynamic imports](https://rollupjs.org/configuration-options/#output-inlinedynamicimports).
- [Service Worker registration](https://developer.mozilla.org/en-US/docs/Web/API/ServiceWorkerContainer/register) — separate secure-origin script URL.
- [PWA installability](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Guides/Making_PWAs_installable).
- [Web app manifest reference](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest).
- [W3C WCAG 2.2](https://www.w3.org/TR/wcag/).
- [HTTP security headers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers).

Related: [Approved ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), [UX contract](../product/web-ux-and-accessibility-contract.md), [web panel stack](web-panel-technical-stack-proposal.md), [approved panel navigation](../decisions/0042-five-section-web-panel-navigation.md).
