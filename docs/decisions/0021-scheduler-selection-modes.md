# ADR 0021 — Scheduler Selection Modes

Status: **Approved**
Date: 2026-10-22

## Decision

Rolvya provides three owner-selectable scheduling preferences at project preflight: **Economical**, **Balanced** (default), and **Performance**. This preference selects how to rank otherwise eligible worker/provider options; it never relaxes security, authorization, capability or hard-budget limits. The owner approved the balanced default and alternatives on 2026-10-22.

### Hard gates always precede ranking

The Rust scheduler first excludes options that fail: immutable DAG leaf readiness and verified dependencies; remote Git conflict safety; authorized model/effort/access mode; approved provider/region; worker identity and sandbox isolation; CPU/RAM/disk and quotas; lease/attempt fencing; enforceable maximum exposure with SQLite-atomic reservation; and reconciled prior launch ambiguity. No mode can bypass a gate.

### Mode semantics

- **Economical:** prioritize conservative expected total cost among suitable options while maintaining minimum quality, completion and reliability requirements.
- **Balanced (default):** jointly consider conservative expected total cost and end-to-end completion time, including provisioning delay, reliability, retries and transfers; avoid disproportionate expense for marginal time savings.
- **Performance:** prioritize estimated completion time among eligible options, still bound by the owner's hard monetary ceiling and other restrictions.

Rank across AI API, compute, storage and transfer costs; duration includes expected provisioning, execution and necessary validation. Performance and economic values are uncertain estimates, not promises. Exact formulas/weights, fairness and aging policy, latency SLOs and concurrency defaults are implementation details pending testing; do not claim arbitrary numeric weights are approved.

### Preflight and runtime

The owner selects a mode or accepts Balanced during preflight; persist in project-approved policy snapshot. Main agent can diagnose and recommend authorized model, effort or provider fallbacks on the same task; Rust scheduler verifies constraints for each new attempt and selects an eligible host using the mode. Do not re-prompt for routine selections. Require explicit owner approval for extra spend, permissions or plan changes. Never add runtime DAG tasks; preserve verified siblings. Audit the chosen provider and rationale.

### Verification

- All three modes reject a cheaper but unsafe, unauthorized or unbounded provider.
- Economic mode chooses lower total cost when other hard constraints hold; Performance favors shorter expected elapsed time within approved cap; Balanced weighs both and provides an explainable choice.
- Multi-provider routing respects selected mode and avoids starvation of eligible lower-priority tasks.
- Budget exhaustion blocks all new billable attempts even in Performance mode.
- Check estimates and outcomes with representative benchmark data before setting numerical weights.

Related: [scheduler selection contract](../architecture/scheduler-selection-policy.md), [preflight approval](../product/preflight-quiet-runtime-policy.md), [budget guard](0010-inflight-budget.md).
