# Documentation Style Guide

This short style guide is tailored for the Maddy Chatmail repo. Use it as a lightweight checklist when editing any Markdown file in `README.md`, `docs/`, `contrib/` and top-level docs.

Voice & tone
- Keep sentences short and direct. Prefer imperative verbs for steps (e.g., "Run", "Create", "Configure").
- Explain "what" and briefly "why" for non-obvious steps.
- Be neutral and factual for reference pages; more friendly and encouraging for tutorials.

Headings
- Use a single H1 per file (already present).
- Use H2 for main sections, H3 for subsections, H4 sparingly.
- Maintain a logical, shallow hierarchy (H2 -> H3 -> H4).

Code blocks
- Always include a language tag: ```bash, ```yaml, ```maddy, ```go, etc.
- Keep examples minimal and copy-paste ready. Avoid long multi-purpose scripts in README; link to docs instead.
- Use comments inside code blocks sparingly and only for clarifying a line.

Placeholders & values
- Use clearly recognizable placeholders: `yourdomain.com`, `/path/to/file`, `X.Y.Z`.
- Avoid real secrets or private values. Use placeholders like `YOUR_API_KEY` when necessary.

Config snippets
- Provide a short explanation above the snippet indicating where it lives (e.g., "Add this to `docker-compose.yml`").
- Where a file must be created, show the exact path and a minimal example.

Admonitions and notes
- Use plain subheadings like "Notes", "Troubleshooting", "Warning"—don’t rely on special macros.
- Keep each note short and place it near the relevant content.

Links and cross-references
- Prefer relative links for internal docs (e.g., `docs/chatmail-setup.md`).
- Make external links explicit and openable: `[Delta Chat](https://delta.chat)`.
- When linking to files that may be moved, prefer linking to a docs page instead of a specific path when possible.

Examples & consistency
- Where the same example appears in multiple files (e.g., Docker Compose), keep one canonical copy in `docs/` and refer to it from README with a short excerpt.
- Use the same placeholder variables across docs (`MADDY_HOSTNAME`, `MADDY_DOMAIN`).

Commit/PR notes for editorial changes
- Keep commits small and focused per file or logical group.
- In PR description, list files changed and the editorial checklist items applied.

Quick checklist (use before finishing an edit)
- [ ] Clear, direct intro added where helpful
- [ ] Headings follow H2/H3 hierarchy
- [ ] Code blocks include language tags and are copyable
- [ ] Placeholders are explicit and consistent
- [ ] Notes/Troubleshooting placed near relevant content
- [ ] Internal links updated and verified

If you'd like, I can now apply this guide to a prioritized batch (starting with `docs/chatmail-setup.md`).