# Secure human browser delivery

G1-08 connects Connected Control to existing HumanAuth, session and project-membership authority through Rust Axum and actual rustls connections. It does not enroll users, bind legacy projects, change budgets, invoke providers or establish production deployment readiness. Base: merged PR #39, `e4c20e728b35ffbd22b999c910df6dbb26855170`.

## Accepted transport and sessions

`human_auth::sessions::https::HumanBrowserServer::open` takes the existing controller bootstrap, a public-PKI-validated `worker_auth::https::tls::ServerIdentity`, canonical HTTPS origin and audited `BrowserAssets`. `serve` accepts TLS directly. Only its private router can create browser facts. The TLS exporter, socket peer, method, target, body commitment, request identity and operation are bound inside that boundary. Forwarded headers, worker bearers and client identity assertions are rejected. Reverse-proxy termination is unsupported until an independently authenticated proxy integration exists.

The adapter enforces exact Host/Origin and same-origin Fetch Metadata, JSON-only mutations, 4 KiB input limits, unknown-field rejection, short request deadlines, bounded connections and separate rate budgets. Per IP/minute defaults are 120 authentication/session mutations, 4,096 authenticated-read attempts and 256 static requests; at most 3,072 address/class entries fit the globally bounded table. These are engineering limits, not new Owner policy. Existing durable credential throttling remains authoritative. Sixteen SSE producers share the connection and request bounds.

The existing issuer delivers `__Host-rolvya` only through an exact, one-use response callback: Secure, HttpOnly, SameSite=Strict, Path=/, no Domain. The accompanying CSRF cookie is also HttpOnly; current session inspection verifies its commitment before returning CSRF material for private client memory. No bearer appears in JSON, React state, URLs or browser storage. Pending factor material stays in a separate HttpOnly cookie. Session inspection rechecks authority transactionally before emitting metadata/CSRF.

Routes below are under `/api/control/v1/`:

| Route | Behavior |
| --- | --- |
| `GET auth/context` | Session-free, short-lived signed browser ceremony and preauthentication CSRF; operational method availability |
| `POST auth/password` | Real Argon2id verification, durable throttling and existing primary-proof/session transition |
| `POST auth/email`, `auth/email/confirm` | Existing verified-delivery challenge lifecycle, login purpose, original browser binding and explicit confirmation |
| `POST auth/factor` | Existing enrolled TOTP or one-use recovery flow; pending login has no panel authority |
| `GET auth/session` | Current identity, assurance, generation and deadlines; no raw session handle |
| `POST auth/renew` | Existing single-winner rotation; no mutation replay after a lost response |
| `POST auth/logout`, `auth/logout-all` | Committed server revocation before claiming sign-out |
| `POST auth/activity`, `auth/interaction` | Short-lived one-use foreground interaction ceremony |
| `GET projects`, `projects/{id}/snapshot` | Existing bounded, redacted session/membership-scoped plan reads |
| `GET projects/{id}/events` | Authorized durable invalidations, not authoritative state |

Production construction installs unavailable mail/delivery adapters. A real sender and independent delivery-evidence adapter still need implementation and qualification; email is not enabled by a configuration claim. It advertises no email method and cannot manufacture delivery or a usable undelivered challenge. Private tests use the existing fake transport. Magic Link opening/preview never consumes a challenge; explicit confirmation in its initiating browser is required. Email expiry remains ten minutes and OTP remains limited to five failures. TOTP is voluntary; an enrolled factor cannot be bypassed. No public signup, reset shortcut, mandatory enrollment or privileged Owner route is added.

Revocation requires the expected family rows to change and the mandatory audit row to exist. Duplicate-friendly primary-proof fencing has a checked postcondition: no matching same-user/session/browser proof may remain unblocked. Ignored or partial SQLite writes roll back logout and return a service error without clearing the cookie or claiming sign-out. Real TLS regressions reproduce both the previous false success and a pending TOTP flow remaining usable; corrected retry revokes those proofs while preserving other users.

## Inactivity, renewal and frontend state

REST reads, SSE/keepalives and renewal are background activity. Visible, focused, online trusted pointer/keyboard input initiates a CSRF-protected interaction ceremony at most once per five minutes. The 256-bit intent is keyed, generation-bound, valid for 60 seconds, and consumed atomically with the idle timestamp. Preparation alone never extends inactivity. Confirmation cannot restore an already expired or revoked session. A bounded expired-intent cleanup does not enable replay.

This validates deliberate interaction with the authenticated application, not independently attested physical human presence. An attacker holding a current cookie and CSRF can imitate that interaction; ordinary frontend checks and zeroization do not defeat controller/browser compromise. The approved seven-day inactivity and existing absolute/token limits remain unchanged. Near-expiry renewal occurs only after confirmed foreground interaction, or through the explicit Session menu. There is no background renewal timer. A lost acknowledgment never triggers another renewal POST or revives the old generation.

The frontend has one authentication state machine, clears private view state on 401/403/revocation, and inspects the real cookie before mounting panel data. Credential-free BroadcastChannel notices coordinate rotation/logout; receiving tabs revalidate the current cookie. The compact email alternative, password visibility, factor/recovery states and help messaging reuse Connected Control Elements. No support address, reset delivery or branding asset is invented. Production never falls back to demo data.

## Read models, events and deployment

The live slice exposes authorized project lists and complete active frozen-plan/task projections. Task runtime success is not verified success without existing R09 acceptance. Financial, detailed attempt, connection, result and activity projections remain disabled; no zeros, invoices, entitlements or outputs are synthesized. Existing Owner financial commands, workers, verifier, Git publisher, holds and DAG are unchanged.

SSE cursors are a checked, monotonic per-project high-water revision from existing append-only scheduler, governance and activation journals. Intermediate revisions coalesce; gaps/future cursors require fresh REST. Migration `0021_human_browser_delivery.sql` adds indexed journal reads and generation-bound one-use interaction metadata. It introduces no session issuer or event/financial ledger; 0001–0020 remain unchanged.

Every stream rechecks session and membership, has a two-frame bounded queue and a 90-second lifetime, and emits only versioned invalidations. Keepalives are comments. Revocation terminates access; cancellation invalidates request authority immediately while capacity remains held until SQLite work completes. Shutdown drains tracked producers. There are no writer locks during network waits. Normal rollover and replay gaps trigger REST revalidation; transport disconnection does not imply worker failure.

The Rust server serves the audited four-file production distribution with correct MIME, script SRI, strict hashed-style CSP, frame restrictions, no-referrer and nosniff. Private APIs are no-store. Only public shell files may enter the existing Service Worker cache; registration also works before sign-in. No offline authentication or mutation queue exists. `browser_panel_routes()` remains a closed bare-router compatibility boundary: mounting it cannot manufacture accepted TLS facts.

## Executable evidence and remaining gates

Tests use genuine SQLite, actual default Argon2id, opaque session/TOTP/recovery authority, scoped memberships, actual loopback TLS and the optimized React build. Browser fixtures are `cfg(test)` only; synthetic enrollment and fake mail are never production configuration. Their temporary credentials are mode 0600 outside uploaded artifacts. Authentication network traces and automatic screenshots are disabled; explicit empty-form captures and CSP/accessibility assertions remain. Browser test certificates are private fixtures, not deployed PKI evidence.

Regression-driven checks include session-response revocation races, unrelated response contexts, concurrent rotation, missing mail, original-browser email binding, cross-project reads, forwarded/worker credentials, malformed/oversized requests, SSE producer capacity under real SQLite writer contention, healthy rollover, live-read rate isolation and one-use interaction rollback. Six actual SIGKILL/reopen barriers cover interaction preparation/consumption and logout before and after commit. Existing session/primary/factor crash regressions remain applicable. Unique test totals, browser engines and exact-head CI are reported separately from nested subprocess barriers and repeated runs.

Deployment still requires independently verified ingress/origin/certificate configuration, securely enrolled humans and project binding, mail delivery where enabled, support/recovery operations and operational restore/reconciliation. Arbitrary unnoticed database rollback is not detected by this adapter. Existing AWS/IAM, provider entitlement, remote isolation and spending activation gates remain closed. PWA icons, physical devices and screen-reader certification remain release gates.

Dependabot #1 is inaccessible/unidentified: the connected GitHub tool does not expose its security endpoint. A separate pinned cargo-audit run still reports unpatched `rsa 0.9.10` / RUSTSEC-2023-0071 in Cargo.lock, absent from the checked Linux normal/build graph. Neither finding is suppressed or declared resolved. Routine integration's stale security-tree receipt is a separate operational issue: the Owner must follow the [protected audit/smoke procedure](pull-request-integration.md), keep routine mode disabled, verify the exact harmless PR/head and record a new successful protected receipt. This change neither alters protection nor dispatches/merges that smoke.

Recognizable runtime fixture secrets are checked against ordinary SQLite, WAL and snapshots. Substring absence does not prove complete memory secrecy, resistance to host administrators, or absence of every possible secret encoding.

Exact-head browser CI also exposed an intermediate theme-switch contrast failure: the shared button background interpolated while its text changed immediately. A frame-sampling regression reproduced 1.06:1 contrast before correction. Button color changes are now atomic; transform motion remains. Accessibility audits still run without rule exclusions. Three controlled-response browser tests explicitly disable Service Workers so Playwright owns their injected transport responses, including WebKit; real authentication, revocation, SSE and PWA tests retain the actual worker. No production cache or trust policy is relaxed.

Rendered empty-form evidence from the optimized application: [light phone view](images/human-browser-login-light.png) and [dark phone view](images/human-browser-login-dark.png). These are actual Linux Chromium renders with external-resource fallbacks, not physical-device certification.

Final local checks pass 922 unique Rust entries, with the one default-ignored browser-driver utility explicitly executed by browser automation and excluded from passing totals. Focused subsets pass 188 human-authentication, 34 finance and 22 migration tests. Frontend checks pass 42 unit/component and 15 artifact/tool cases, plus 36 Chromium browser cases; repository checks pass 203 Node and 23 Python tests. Six nested SIGKILL barriers are not six additional unique tests. Final-head CI and additional browser-engine evidence are recorded separately in the PR.
