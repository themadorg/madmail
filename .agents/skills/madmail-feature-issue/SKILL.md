---
name: madmail-feature-issue
description: Draft or create Madmail feature request issues with the gh CLI, following the repository's feature issue form and contribution documentation. Use for filing enhancements in themadorg/madmail, rather than implementing features or reporting bugs.
---

# Madmail feature issues

Turn the user's feature idea into a concrete issue in `themadorg/madmail`. Preserve whether they asked for a draft or publication. Creating this skill does not authorize filing an issue; an explicit request to file an issue authorizes submission without another confirmation. Do not implement the feature, assign people, or add projects or milestones unless requested.

## Discover current requirements

- Confirm the target with `git remote -v` and `gh repo view --json nameWithOwner,url`. Pass `--repo themadorg/madmail` explicitly on issue commands so a fork or the admin-web submodule is not targeted accidentally. For an explicitly requested different repository, use its own template instead of Madmail's format.
- Read `.github/ISSUE_TEMPLATE/feature_request.yml`, `.github/ISSUE_TEMPLATE/config.yml`, `CONTRIBUTING.md`, and relevant parts of `docs/project/17-extend-and-contribute.md`. Find relevant design material in `docs/TDD/` and `docs/plans/`. These sources take precedence over the snapshot below.
- Without a checkout, fetch sources with `gh api repos/themadorg/madmail/contents/PATH --jq .content` and base64-decode the result. Never invent a missing template.
- Search open and closed issues for the capability and related terms, e.g. `gh issue list --repo themadorg/madmail --state all --search 'search terms' --limit 30 --json number,title,url,state`. Inspect likely matches with `gh issue view`. A result limit is not evidence that all issues were checked. If an existing issue covers the request, return its link and explain the overlap; do not create a duplicate or comment without authorization.

## Research before posting

Research the project and existing issues before composing the final issue or running `gh issue create`. Reading the issue template alone is insufficient.

- Trace the relevant capability through project documentation, implementation, configuration/CLI or API entry points, and existing tests. Establish what already works, what is missing, and which components the proposal would affect. Check the checkout's revision against the repository's current default branch before claiming a feature is absent; inspect current remote sources when the checkout is stale.
- Search issues using both the user's wording and project terminology, including synonyms and relevant component names. Review the bodies and comments of likely matches with `gh issue view NUMBER --repo themadorg/madmail --comments`. Follow linked issues and pull requests when they explain an existing implementation, planned work, or a previous rejection. Broaden or paginate searches when results are truncated.
- Use findings to refine the problem, solution, alternatives, scope, and constraints. Distinguish verified current behavior, user-reported behavior, and proposed behavior. Link the relevant existing issues, design documents, and source locations in the body where they support a claim; summarize why related work does not already cover the request.
- If the capability already exists, explain how to use it. If an issue already covers it, return that issue. If essential project or issue research is unavailable, provide a draft with the research gap and leave publication pending rather than posting unsupported claims.

## Compose the issue

The checked template uses title prefix `[Feature]: ` and label `enhancement`. Also set issue type `Feature` when supported: verify with `gh api repos/themadorg/madmail/issue-types` and installed CLI help. Labels and issue types are distinct. Render its fields as Markdown headings in this order:

```markdown
### Area

<one current template option>

### Problem

<observed gap, affected users or operators, and why it matters>

### Proposed solution

<desired behavior and relevant operator or Delta Chat user impact>

### Alternatives considered

<actual alternatives or workarounds; say “Not discussed” if none are known>

### Estimated scope

<one current template option, with a brief rationale if useful>

### Design constraints

- [ ] Should preserve behavior parity with Madmail v1 (Go) where applicable
- [ ] Affects PGP enforcement, auth, admin tokens, or logging (No-Log)
- [ ] Requires a new external process or mandatory sidecar
```

Current area options: SMTP; IMAP; Federation; Authentication / registration; Admin API / admin web UI; TURN / STUN / Iroh relay; TLS / ACME; Storage / quota; Configuration / CLI; Operator / deployment experience; Documentation; Other.

Current scope options:

- Small (localized change, one crate or doc)
- Medium (cross-crate, new API surface, new tests)
- Large (new subsystem — likely needs TDD / plan doc first)

Area, Problem, Proposed solution, and Estimated scope are required. Alternatives and constraint checkboxes are optional in the form; retain their headings for a consistent CLI-created body. Estimate scope from available evidence. Check only constraints supported by the proposal; explain unknown impact in prose instead of treating unchecked boxes as verified absence.

Keep the problem separate from implementation suggestions. Include concise, observable acceptance criteria under Proposed solution when they clarify completion. Link existing design notes and related issues. For large changes, identify whether a TDD or plan is needed; filing an issue does not require writing that design. Preserve the single-binary/simple-operations aim and document intentional v1 differences. Do not invent code findings, reproduction results, alternatives, or completed checks. Ask for missing information only when the core problem or desired behavior cannot be reasonably inferred. Route bug-only reports to the bug form and undisclosed vulnerabilities to `SECURITY.md` rather than a public feature issue.

## Submit with gh

Write the exact body to a temporary UTF-8 Markdown file using a file tool or properly quoted heredoc. YAML issue forms do not validate a CLI-authored Markdown body: explicitly check the title prefix, required sections, area/scope choices, and constraints before submission. Consult `gh issue create --help` for installed CLI behavior; do not rely on `--template` to populate the form.

For a draft-only request, return the title and body without creating an issue. For an authorized submission:

```bash
gh issue create --repo themadorg/madmail \
  --title '[Feature]: Concrete requested capability' \
  --label enhancement --type Feature \
  --body-file /absolute/path/to/feature-issue.md
```

Pass the actual title as a safely quoted argument. Do not create labels or change repository settings if metadata is missing. After creation, verify the returned issue with `gh issue view URL --json title,body,labels,url` and return its link. If creation times out or reports an ambiguous failure, search for the exact title and inspect any returned URL before retrying; stop if creation status remains uncertain to avoid duplicates.

Verify issue type using `gh api repos/themadorg/madmail/issues/NUMBER --jq .type`. For bug reports use the neighboring `madmail-bug-issue` skill. For requested additional labels, project membership, priority, effort, or other fields use `madmail-issue-tags`; do not infer those values from a screenshot example. Missing project access does not block separately authorized issue creation, but report any requested metadata left unapplied.
