---
name: caveman
description: Keep an agent's interactive progress notes and final summary concise without dropping technical evidence. Use when requested to minimize response tokens or explicitly invoking Caveman; never for rewriting code, documentation, PR bodies or audit records.
---

# Caveman — Rolvya-safe concise communication

Adapted for Rolvya from [Caveman](https://github.com/JuliusBrussee/caveman/blob/9b68f7ed12b05e7efaff18e65c5a3fc2849fd0a4/skills/caveman/SKILL.md) (Apache-2.0; source revision `9b68f7ed12b05e7efaff18e65c5a3fc2849fd0a4`). This is a self-contained, reviewed adaptation; it does not install the upstream runtime, hooks, proxy, compression scripts or packages.

- Only shorten **conversational narration, progress notes and human-readable summaries**. Keep technical content and essential caveats intact.
- State outcome first; use compact sentences. Remove greeting, empty praise and repeated recaps; preserve the user's chosen language and legible grammar.
- Do not alter exact commands, API names, paths, URLs, code blocks, error messages, numbers, test results, security qualifiers or mandatory negative terms such as *not*, *never*, and *only*.
- Never condense `AGENTS.md`, ADRs, contracts, migration plans, architecture decisions, code comments, commit/PR messages, review findings or audit logs. They are persisted records and must remain complete.
- All substantive handoffs still include objective, files/commits, decisions, actual tests, skipped tests, risks, spend and next task per `AGENTS.md`.
- Expand clearly when explaining a risky operation, security limitation, irreversible action or failed validation.
- Avoid a persistent mode assumption across new Codex Cloud tasks. Activate only when the current user/task requests terse communication, and stop when asked.
- No shell tools, data transmission, automatic file writes or agent delegation belong to this skill.

Upstream source: https://github.com/JuliusBrussee/caveman/tree/9b68f7ed12b05e7efaff18e65c5a3fc2849fd0a4/skills/caveman
