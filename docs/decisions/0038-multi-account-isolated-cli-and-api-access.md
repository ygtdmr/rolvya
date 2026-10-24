# ADR 0038 — Multi-account isolated CLI sessions and API-key access

Status: **Approved architecture**  
Date: 2026-10-24

## Decision

Rolvya **supports both genuine coding CLI sessions and API-key access**. Its remote multi-agent system must allow **multiple distinct provider accounts to be connected at once**, each with an independently authenticated, isolated CLI environment (for example Claude Code account A, Claude Code account B, Codex account C), with eligible agents running concurrently across accounts. This is the Owner's explicit refinement to OQ-008, inspired by Paperclip's connection and isolated-login architecture.

### Login and account isolation

1. **Real official CLI login:** A user starts a connection through Rolvya's web UI, selects Claude Code or Codex and completes the **provider-supported interactive browser/device-code login** for that account. The result is a real supported CLI login, not a simulated terminal identity, a spoofed official application, or a reused personal-computer session.
2. **Remote-only sign-in:** Use Rolvya-controlled isolated **remote sign-in environments** (or other officially supported remote auth flows). Do not require the user's personal computer to run a Rolvya agent, upload CLI auth files, sync repositories, or host workers. Browser-based consent performed by the user is allowed and is distinct from local-device access.
3. **Independent connections:** Track each provider account as an explicit connection with opaque ID, authorized Rolvya user, provider, method (CLI subscription or API key), capabilities, status and permitted projects/agents. A connection must not inherit the host's default login, another tenant's auth directory, another user's credentials or an unintended environment-variable API key. Account connection, revocation and reconnect do not automatically switch other connections.
4. **Isolated credential homes:** Every concurrent CLI execution receives a separate restricted runtime home, filesystem/workspace and credential scope. Login session grants are kept encrypted per account/connection by Rolvya's trusted Secret Broker; runtime projection is isolated and allowed only for that approved connection/attempt. Do not assume copying a rotating refresh token across multiple worker homes is safe. Where a provider requires serialized token refresh or prohibits concurrent sessions, enforce that constraint instead of sharing mutable tokens unsafely.
5. **Concurrent multiple accounts:** Scheduling may use independently eligible connections in parallel, subject to real provider terms, account quota/concurrency restrictions, project authorization, worker isolation and the immutable up-front planned model/effort/access mode. Scheduler chooses among **pre-authorized eligible accounts** without changing the frozen task DAG or silently switching providers, modes or paid accounts. User explicitly controls optional sharing; account ownership alone never creates another user's access.
6. **API key as a first-class separate mode:** A user can configure API keys alongside CLI account connections. API-key work uses the separately approved broker/minimum-scope path, declared pay-as-you-go cost and hard pre-reserved budget. A CLI session is not an arbitrary API credential; an API key must not unexpectedly override a selected subscription mode.
7. **No silent fallback or quota evasion:** If an account needs login, loses access, reaches quota, exceeds allowed concurrency or cannot legally run unattended remotely, mark that account/mode unavailable and hold affected attempts. Never silently use another account or pay-as-you-go API, rotate accounts to evade provider limits, emulate an official client's identity or bypass subscription usage policy.
8. **Governance and metering:** The main planner fixes task model, effort and allowed access mode up front. Rust/SQLite holds accountable connection selection, provider eligibility, lease/fencing, audit and separate API/compute/storage/transfer cost reporting. A user may disconnect/revoke their connection and see which agents can use it. Actual third-party usage remains subject to provider authorization and an enforceable Rolvya budget.

### Provider-specific boundary: not yet granted

**Approval of this capability is not proof that any provider allows every third-party hosted, unattended or subscription-backed CLI use case.** Verify the actual integration and account entitlement before enabling that runtime. Anthropic's current published guidance prefers Console/cloud-provider API authentication for third-party tools and prohibits impersonation or unauthorized subscription-limit routing. OpenAI documents official Codex CLI device login and selected Sign in with ChatGPT third-party plan-use paths, but remotely hosted/paid uses can have separate eligibility/approval requirements. Do not treat Paperclip's implementation as a provider license.

If subscription-backed remote CLI operation is unsupported, the **architecture stays supported but that specific mode remains blocked** until an official permitted integration exists. API-key mode can still run when separately authorized; **never auto-convert a failed subscription login into API charges**.

### Deferred implementation specifics

Exact UI screens, credential encryption backend, per-provider login runner, token refreshing/serialized grants, remote host/sandbox details, process invocation, permission matrix and quota algorithms are implementation work. No numeric account limit or universal cross-account fallback policy was approved. All current worker token/lease/heartbeat defaults, hard budget gates, remote-only prohibition on local agents and the Secret Broker ([ADR 0037](0037-central-secret-broker-task-scoped-access.md)) remain unchanged.

## Future acceptance tests (unexecuted)

- Connect two Claude Code accounts and one Codex account separately with provider-supported flows; each has its own identity, connection ID and credential home.
- Run eligible independent tasks simultaneously under different account connections without cross-account credentials, prompts, filesystem history or billing confusion.
- A user disconnecting/reconnecting one account cannot access or overwrite another's CLI auth state.
- An unauthorized project/agent, stolen worker token or injected host API-key variable cannot select or silently override a connection.
- Subscription mode blocked by terms, missing consent, quota or revoked session does not fall back to a different account or billed API without explicit authorization and a new allowed cost reservation.
- Each CLI runtime's provider-specific refresh and concurrency policy is respected; avoid shared mutable auth-file race/replay.
- API key and CLI subscription billing/usage reported as distinct modes; both obey approved budget rules.
- No Rolvya local agent or personal-computer file/credential extraction is required.

Related: [OQ-008 contract](../architecture/model-access-entitlement-contract.md), [dated official evidence](../reference/provider-cli-api-policy-evidence.md), [worker security](0037-central-secret-broker-task-scoped-access.md), [open questions](../project/open-questions.md).

**Status: approved product/architecture direction only; no multi-account implementation or live account test is claimed.**
