# Rolvya agent skills

These repository-scoped Codex skills add focused working procedures; they do not change Rolvya product architecture or install executables. The existing Graphify integration is under `.codex/skills/graphify/`; do not replace it.

## Rolvya-native skills

- `rolvya-rust-foundation` — Rust domain foundations and immutable task planning.
- `rolvya-sqlite-integrity` — SQLite transaction and budget/secret safety.
- `rolvya-verification` — deterministic failure-path and CI evidence.
- `rolvya-connected-ui` — cohesive React/Vite UI and build artifacts.
- `rolvya-pr-risk-review` — behavior, authorization and merge-risk review.

## Externally informed, reviewed adaptations

These are **self-contained Rolvya-safe adaptations** of specific open-source skills, **not a wholesale copy, installation or execution of the original toolkits**. Each `SKILL.md` credits its pinned upstream source revision and preserves the external project's attribution.

| Local skill | Source | Activation |
| --- | --- | --- |
| `caveman` | [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman/tree/9b68f7ed12b05e7efaff18e65c5a3fc2849fd0a4/skills/caveman), Apache-2.0 | Explicitly requested concise responses only |
| `systematic-debugging` | [obra/superpowers](https://github.com/obra/superpowers/tree/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/systematic-debugging), MIT | Actual bug or failed test |
| `test-driven-development` | [obra/superpowers](https://github.com/obra/superpowers/tree/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/test-driven-development), MIT | New behavior or regression fix |
| `verification-before-completion` | [obra/superpowers](https://github.com/obra/superpowers/tree/2ce8aa15d05bd57213e6f746f953aafb7ded7a8b/skills/verification-before-completion), MIT | Completion claims or PR handoff |
| `react-best-practices` | [vercel-labs/agent-skills](https://github.com/vercel-labs/agent-skills/tree/063bee94c3f4df8453406c830b0a7df0f2860278/skills/react-best-practices), upstream MIT declaration | React rendering, state and bundle |
| `web-design-guidelines` | [vercel-labs/agent-skills](https://github.com/vercel-labs/agent-skills/tree/063bee94c3f4df8453406c830b0a7df0f2860278/skills/web-design-guidelines), upstream MIT declaration | UI/UX and accessibility review |

### Safety and precedence

1. `AGENTS.md`, approved ADRs, hard budget approval, security rules and `docs/project/status.md` outrank all external advice. No external skill may turn a proposed design into an approved one.
2. Do not install or execute third-party scripts, hooks, proxies, `npx` installers or npm packages automatically. No outbound API/model calls for skill operation. These six adaptations are local Markdown only.
3. Never apply Next.js or SSR-specific techniques that violate approved React + TypeScript/Vite, Rust Axum or the exactly-one-app-JS-containing-CSS artifact contract.
4. Use Graphify for architecture/code relationship exploration, but verify with source and tests. Update graph after source modifications.
5. External skills cannot approve, merge or widen the routine integration policy. Critical changes still require Owner review. An agent skill does not provide a distinct human identity.
6. Review future upstream releases and license changes before selectively updating an adaptation. Treat the pinned revision as provenance, not a live auto-update or an independently installed binary.
7. After this PR lands, start a **fresh Codex Cloud task** to verify project skill discovery/invocation. A valid local file and green GitHub CI do not themselves prove Codex has loaded the skill.
