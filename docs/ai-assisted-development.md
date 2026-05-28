# AI-assisted development

> **Work in progress** — this document will describe how Madmail v2 was developed with human direction and AI tooling.

## Tools used

| Tool | Role |
|------|------|
| **[Cursor](https://cursor.com)** (coding agent) | Day-to-day implementation: editing Rust crates, tests, docs, and scripts in-repo |
| **[Gemini 3.1 Pro](https://aistudio.google.com/)** (Google AI Studio) | Up-front and ongoing **planning**: phased roadmaps, gap analysis, and large-context review using bundles such as `context.txt` |

## Related material

- Planning prompts: [`docs/prompts/`](prompts/planing-prompt.md)
- Context bundle generator: [`scripts/build-planning-context.sh`](../scripts/build-planning-context.sh)
- Reference trees fed into planning: [Context & reference projects](context-references.md)
- Build narrative (phases, tickets, gates): [How Madmail v2 was built](how-we-built-it.md)

## Human vs AI

The **goals, architecture, security model, and phase boundaries** were set by humans. AI agents produced and revised most of the implementation code under that guidance; humans reviewed, ran tests, and corrected behavior against Madmail v1 and Delta Chat clients.

See the **Disclaimer** in the root [README](../README.md#disclaimer).
