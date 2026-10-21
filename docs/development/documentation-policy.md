---
title: Documentation and Obsidian Compatibility
status: Approved
category: development
last-reviewed: 2026-10-21
tags:
  - documentation
  - obsidian
  - markdown
  - agent-workflow
---

# Documentation and Obsidian compatibility

## Source of truth

The GitHub repository is the canonical source of project documentation. Obsidian is an optional Markdown editor and knowledge-graph viewer, not a separate authoritative database. A reader or AI coding agent must be able to understand the documentation without installing Obsidian.

## Compatibility rules

1. Write portable UTF-8 Markdown, with a single descriptive H1, clear section headings and relative file links. All internal navigation **must work on GitHub**. Prefer `[Architecture](../architecture/overview.md)` over Obsidian-only `[[wikilinks]]`.
2. Use portable YAML frontmatter when metadata is useful. Recommended keys: `title`, `status`, `category`, `last-reviewed` (ISO date), `tags`. Avoid plugin-specific required properties. Metadata supplements, but does not replace, readable text.
3. Use **file-relative** paths for cross-file references, resolving them from the directory containing the referencing file. Do not link to undocumented/nonexistent targets as though they already exist.
4. Use Mermaid fenced code blocks for diagrams where appropriate, with a text explanation and explicit direction of dependencies. Diagrams must not be the sole source of normative requirements.
5. Prefer ordinary Markdown tables, code fences and links over Obsidian-only embeds, callouts or proprietary plugin syntax. Avoid duplicate note titles that could create ambiguous references.
6. Organize notes by ownership: `docs/product/`, `docs/architecture/`, `docs/operations/`, `docs/development/`, `docs/project/`, `docs/decisions/`, `docs/reference/`. Link related topics deliberately so Obsidian Graph View is useful.
7. Keep documents self-contained enough for machine readers. Link to a single authoritative specification instead of copying normative text across files. Distinguish **Approved**, **Proposed**, **Open**, **Implemented** and **Verified**.
8. A change to code, specifications or approved decisions must update directly affected documentation, links and project status in the same reviewable change.

## Obsidian Vault setup

Open the cloned repository **root** as an Obsidian Vault. Opening only `docs/` hides root-level `README.md` and `AGENTS.md`, so links to those files will not resolve from that partial vault. Markdown files remain ordinary tracked repository files; `.obsidian/` settings, plugins and caches are not required for contributors or agents. Avoid committing personal workspace settings or secrets.

## Review checklist

Before accepting documentation changes:
- All modified relative links resolve within the repository.
- Frontmatter (if present) parses as valid YAML; tags and status are consistent.
- Markdown and Mermaid remain readable in GitHub's renderer.
- Obsidian can navigate the same notes without a required community plugin.
- An agent can distinguish established decisions from future goals and completed code from unimplemented plans.
