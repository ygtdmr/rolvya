---
title: Provider Coding-Agent CLI API Entitlement Evidence
status: Proposed
category: reference
last-reviewed: 2026-10-22
tags: [claude, codex, cli, api, subscriptions, terms]
---

# Provider coding-agent CLI/API entitlement evidence

## Why this reference matters

Rolvya's historical user request included **Claude Code CLI and Codex CLI**, subscription-only use where appropriate and API-key mode when configured. These desired product modes do **not** establish technical or contractual permission to pool subscriptions, reproduce a native app's OAuth identity in a third-party service, or run unlimited paid worker sessions on remote cloud machines.

**Rolvya should not enable unattended remote subscription mode until the relevant official provider documentation and actual user entitlement have been reviewed for that particular integration.** This page records evidence checked on **2026-10-22**; terms/models/prices can change. It is not legal advice or an implementation claim.

## Official provider evidence

### Anthropic / Claude Code

- [Claude account sign-in policy](https://support.claude.com/en/articles/13189465-log-in-to-your-claude-account) describes subscription authentication for native Anthropic apps, including Claude Code, and identifies **API key access through Claude Console or supported clouds as the preferred way for third-party tools/services**. The page warns against tools misrepresenting their identity or routing third-party traffic improperly against subscription limits. This directly affects a third-party open-source Rolvya orchestration product.
- [Claude Code model usage and limits](https://support.claude.com/en/articles/14552983-models-usage-and-limits-in-claude-code) distinguishes subscription/seat usage limits from API pay-as-you-go charging.
- [Claude Code setup](https://docs.anthropic.com/en/docs/claude-code/getting-started) documents Console/API, Claude subscription and enterprise cloud authentication routes but **does not itself grant permission to reuse a subscription in an arbitrary third-party remote orchestration service**.

**Rolvya implication:** the user-preferred subscription-only Claude CLI path is a *requested product behavior conditional on official allowance*, not automatically eligible. A valid subscription alone is insufficient evidence. When eligibility cannot be proven, block setup/dispatch and explain available authorized API or supported alternatives **without switching to a paid API on the user's behalf**.

### OpenAI / Codex

- [Codex CLI official repository](https://github.com/openai/codex) describes ChatGPT account sign-in and separate API-key setup.
- [Codex CLI non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode) documents automation guidance and favors **API key authorization** for many automated contexts, including strict credential handling in CI; ChatGPT-managed authentication has additional constraints.
- [Codex CLI developer commands](https://learn.chatgpt.com/docs/developer-commands) documents distinct sign-in options and inspecting active CLI authentication mode.
- [Codex Sign in with ChatGPT app-server](https://developers.openai.com/siwc/token-sharing-open-source/codex-app-server) documents a particular OAuth/token-sharing architecture for qualified integrations; **a documented flow is not blanket authorization for Rolvya to reuse a user's tokens**.
- [Sign in with ChatGPT errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery) explicitly discusses consent and failure behavior; never fall back to other billable modes invisibly.

**Rolvya implication:** preserve user-selected billing/auth mode, validate consent/scope, separate remote worker credentials from model context, treat unsupported/expired token as blocker, and do not assume sign-in on one device authorizes all remote worker hosts. The user's historical subscription-only **Claude CLI** preference does not automatically select Codex CLI.

## Paperclip reference and official multi-account remote login evidence — 2026-10-24

- [Paperclip OpenAI connector](https://github.com/paperclipai/paperclip-docs/blob/main/docs/connectors/openai.md): CLI subscription or API key, separate per-login `CODEX_HOME`, optional host or interactive sandbox sign-in, connection assignment to eligible agents.
- [Paperclip AI Connections](https://github.com/paperclipai/paperclip/blob/master/doc/connections/AI-CONNECTIONS.md): separate protected connection grants/private runtime homes, permission checks and careful token refresh; warns against importing the host operator's existing rotating-token file.
- [OpenAI Codex CLI authentication](https://learn.chatgpt.com/docs/auth): official browser sign-in and device code for remote/headless login; credentials are sensitive. API keys remain recommended for automation.
- [OpenAI Sign in with ChatGPT OSS capability](https://developers.openai.com/siwc/token-sharing-open-source): explicit consent/registration and account profiles; **paid or remotely hosted apps require separate access/eligibility**. Rolvya must not infer it has provider approval merely because the SDK exists.
- [Anthropic Claude subscription and third-party policy](https://support.claude.com/en/articles/13189465-log-in-to-your-claude-account): API/cloud access is preferred for third-party tools, while subscription access is meant for genuine native usage. No native-client impersonation or subscription-limit evasion.
- [Anthropic CLI environment priority](https://support.claude.com/en/articles/12304248-manage-api-key-environment-variables-in-claude-code): inherited API-key environment variables can override a subscription login and generate unexpected pay-as-you-go charges.

**Approved Rolvya design:** multiple independently authenticated CLI accounts in isolated remote execution environments, plus API-key connections ([ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md)). **Still P0:** actual rights to run each subscription via third-party unattended remote orchestration, account/workspace consent, quota, safe token renewal and hosted installation conditions. A real CLI login is not by itself a license for prohibited use.

## Verification checklist before writing live adapters

| Requirement | Evidence required |
| --- | --- |
| Provider terms allow the **specific** third-party, unattended remote-worker mode | Current official policy/legal review date and URL |
| Subscription vs API key mode explicitly selected | Authenticated owner configuration/consent, no silent fallback |
| CLI supports the remote OS and noninteractive invocation | Versioned official command support and isolated test |
| Secure credential storage and rotation | Secret references, no raw tokens in task prompts/Git/logs |
| Concurrency/session/device limits | Provider account-specific documented entitlement and controlled test |
| Model/effort availability | Capability check, current provider model catalog, successful authorized request |
| Cancellation/usage ceiling | Enforceable bound or Approved ADR 0010 stop-on-risk |
| Usage accounting | API actuals vs subscription quota/known allocation clearly distinguished; compute/storage/transfer always tracked |
| Licensing or automation is ambiguous | Leave feature blocked/Proposed; do not spoof native app identity |
| Permission is revoked or terms change | Fail closed, audit, show reauthorization requirement |

## Example product UI behavior (proposed)

- **CLI / subscription:** “This access mode requires entitlement verification for remote unattended execution. Subscription authentication alone does not guarantee support.”
- **API:** “Use an API key only if you explicitly authorize this access mode and its pay-as-you-go charges.”
- **Unavailable:** “No verified authorized mode for this planned task; execution has not started.”
- **Retry:** “A different model, effort, account or provider requires an allowed fallback and fresh budget reservation.”

Do not expose raw OAuth or API keys in warnings, logs or browser responses.

## Separation from architecture decisions

**Approved:** fixed task DAG and per-task model/effort assignments; Rust/SQLite control; HTTPS workers; provider-neutral host adapters; strict human-approved budget; remote-only execution. **Not decided:** specific Anthropic/OpenAI licensing workaround, exact CLI versions and models, subscription delegation, entitlement verification UI, token-sharing architecture, default mode when both subscription and API keys are present.

If a capability is technically possible but unlicensed or unauthorized, Rolvya must **not** enable it merely to satisfy a historical preference.

Related: [model access contract](../architecture/model-access-entitlement-contract.md), [model access overview](../architecture/model-access.md), [secrets](../operations/secrets.md), [security](../architecture/security.md), [open questions](../project/open-questions.md).

## Official provider recheck — 2026-10-24 (OQ-008; source evidence only)

- [Anthropic — Claude account login/subscription versus third-party tools](https://support.claude.com/en/articles/13189465-log-in-to-your-claude-account): Claude's native subscription use includes Claude Code, but **third-party tools should use a Console or supported cloud API key**. Third-party software cannot impersonate a native client or assume subscription quota may finance unattended external automation. Provider-managed exceptions/usage credits are not blanket Rolvya entitlement.
- [Anthropic — Claude Code subscription and API key priority](https://support.claude.com/en/articles/12304248-manage-api-key-environment-variables-in-claude-code) and [Pro/Max account usage](https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan): API-key environment authentication may take precedence and incur separate pay-as-you-go charges; avoid hidden environment credentials and verify actual mode.
- [OpenAI — Using Codex with your ChatGPT plan](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan): Codex CLI supports ChatGPT sign-in and account-specific limits. [ChatGPT Work and Codex](https://help.openai.com/en/articles/20001275-chatgpt-work-and-codex) distinguishes ChatGPT-plan usage/billing from API-key pricing. This does **not** automatically demonstrate consent or approved unattended credential forwarding to arbitrary Rolvya-controlled third-party worker hosts.
- [OpenAI — Using your ChatGPT plan in other apps/sites](https://help.openai.com/en/articles/20001542-using-your-chatgpt-plan-in-other-apps-and-sites): certain participating integrations may offer explicit third-party subscription use and usage limits, but Rolvya must not presume it is a participating/approved app or copy unrelated tokens.

**Implication:** CLI executable presence or a user's personal subscription alone is insufficient. Allow **API-key remote operation only after explicit authorization, provider eligibility, protected broker access and hard budget reservation**. Allow unattended subscription-backed third-party CLI mode **only with documented provider-permitted integration, user consent and secure credential/host policy**. Otherwise block, preserving the historical subscription-only Claude CLI preference as an unmet target rather than secretly switching to an API bill. This note makes **no new provider integration Approved**.
