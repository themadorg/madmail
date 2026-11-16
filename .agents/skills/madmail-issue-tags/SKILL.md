---
name: madmail-issue-tags
description: Research and manage Madmail GitHub issue labels, issue types, and requested project fields using gh CLI. Use for issue tagging or metadata triage, not Git release tags or publishing releases.
---

# Madmail issue tags and fields

“Tags” here means GitHub issue labels. Issue type, organization fields, and project fields are separate metadata. A screenshot can identify intended fields but is not evidence that every issue should receive its values. If the user means Git release tags, clarify and do not use this workflow.

## Research and select

Confirm the repository and target issue numbers/URLs. Read each issue body, comments, and relevant project docs/code before classifying it. Inspect related issues and PRs to understand scope, duplication, resolution, and current conventions. For a bulk request define the target set from the user's scope; do not extend it to the whole repository implicitly.

Discover live labels and types:

```bash
gh label list --repo themadorg/madmail --limit 100
gh api repos/themadorg/madmail/issue-types
gh issue view NUMBER --repo themadorg/madmail --comments
gh api repos/themadorg/madmail/issues/NUMBER
```

Paginate if needed. Use label descriptions and `.github/ISSUE_TEMPLATE/` as primary guidance. Feature requests use `enhancement` and type `Feature`; bug reports use `bug` and type `Bug`; general tasks may use `task` and type `Task` if supported by current conventions. The separate label `feature` is not a substitute for the feature template's `enhancement`. Apply component labels such as `frontend`, `backend`, or `ui/ux` only when supported by the issue. Do not mark `released`, `duplicate`, `wontfix`, or `good first issue` without evidence. Never expose private security details through public metadata.

Preserve existing unrelated labels, assignments, and fields. A request to apply or triage tags authorizes relevant metadata edits; a recommendation-only request does not. Creating these skills does not authorize any issue edits. Do not create/rename/delete repository labels, close issues, assign people, or set milestones without that scope.

## Apply labels and types

Use additive edits and remove labels only when requested or demonstrably incorrect within authorized triage:

```bash
gh issue edit NUMBER --repo themadorg/madmail --add-label enhancement --type Feature
```

Use `--remove-label NAME` for a justified removal. Inspect `gh issue edit --help` for installed flags. If type flags are unavailable, use `gh api --method PATCH repos/themadorg/madmail/issues/NUMBER -f type=Feature` with the discovered supported type name. Verify labels/type through the issue REST endpoint after editing. Do not overwrite the whole label list from a stale read.

## Projects and fields when requested

The reference screenshot shows project `madmail-v2`, Status `Todo`, Priority `Medium`, Effort `Medium`, and unset dates. Treat these as an example, not universal defaults. Priority and Effort may be organization issue fields or project fields; discover which before writing. Do not turn them into labels.

Inspect `gh project list --owner themadorg --format json`, the chosen project's `gh project field-list NUMBER --owner themadorg --format json`, and its items with `gh project item-list`. Use `gh project --help` and subcommand help to confirm the installed syntax. Discover actual project, item, field, and option IDs instead of hardcoding them or confusing issue numbers with project item IDs. Add project membership only when requested; check existing membership before adding. Use `gh project item-edit` for supported project fields. For organization issue fields use the supported GitHub API discovered from the live schema/documentation; do not guess endpoints or mutate project fields as a substitute.

The current token was observed to lack `read:project`; project writes also require appropriate `project` access. Recheck access when needed. If unavailable, complete separately authorized label/type work and report the precise unapplied fields and access error. Do not refresh authentication or broaden scopes automatically. Missing metadata access alone need not block separately authorized issue creation; disclose partial completion.

Verify each changed value from a fresh read and return links with a concise account of changes and any unapplied fields. After a timeout read current state before retrying mutations; stop if the state remains uncertain.
