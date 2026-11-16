---
name: madmail-bug-issue
description: Research and draft or file Madmail bug reports with gh CLI using the repository bug template. Use for unexpected behavior in themadorg/madmail, rather than feature requests or implementing fixes.
---

# Madmail bug issues

Honor draft versus publication intent. A request to file a bug authorizes that issue; creating this skill does not. Do not implement fixes or change unrelated metadata.

## Research before posting

Confirm the repository with `git remote -v` and `gh repo view --json nameWithOwner,url`. Explicitly target `--repo themadorg/madmail`; for an explicitly different repository use its own conventions.

Read `.github/ISSUE_TEMPLATE/bug_report.yml`, its `config.yml`, `CONTRIBUTING.md`, `SECURITY.md`, and relevant project/TDD documentation. Inspect the relevant implementation, entry points, configuration, and tests. Compare the reported version with the current default branch and related fixes before describing current behavior. Without a checkout, use `gh api repos/themadorg/madmail/contents/PATH --jq .content` and base64-decode sources.

Search open and closed issues using the symptoms, exact error, component names, and synonyms: `gh issue list --repo themadorg/madmail --state all --search 'terms' --limit 30 --json number,title,url,state`. Broaden or paginate truncated results. Read likely matches and comments with `gh issue view NUMBER --repo themadorg/madmail --comments`; follow relevant linked PRs. Search relevant discussions as well, using `gh api graphql` if needed, because the reporter checklist explicitly includes them. Return a covering existing issue instead of duplicating it; do not comment without authorization.

Distinguish user reports, verified observations, and suspected causes. Reproduce only when safe in an isolated local environment; never run destructive commands or probe production to fill a report. Do not claim reproduction or test success without performing it. If essential research is unavailable, leave a draft and explain the gap. Undisclosed vulnerabilities, including auth bypass, PGP enforcement failure, or data leakage, follow SECURITY.md and must not be posted publicly.

## Compose from the current form

Use `[Bug]: ` followed by the concrete symptom, label `bug`, and issue type `Bug` when supported. Preserve these headings in order, taking current labels/options from the YAML rather than a stale example:

```markdown
### Area

<one exact current area option>

### Madmail version

<reported madmail version output or release tag>

### What happened?

<actual behavior, expected behavior, impact; separate evidence from hypotheses>

### Steps to reproduce

<minimal known steps; clearly state if not reproduced independently>

### Environment

<known OS, architecture, deployment mode, client version and relevant sanitized configuration>

### Logs / error output

<sanitized relevant output in a fenced code block, or state none provided>

### Reporter checklist

- [ ] I searched existing issues and discussions for duplicates
- [ ] I redacted secrets, admin tokens, and personal data from logs
```

Area, version, description, reproduction steps, and environment are required by the current form. Ask for essential missing reporting details; do not substitute the local development version for the affected deployment version. Keep a draft if required facts remain missing. Logs are optional. Check reporter boxes only after actually completing those checks. Redact passwords, tokens, private addresses, and personal data, including quoted commands and attachments. Link relevant issues, fixes, docs, and source locations where they support the report; explain why related work does not cover it.

## Submit and verify

For draft requests return the title/body. For authorized publication, write exact UTF-8 Markdown using a file tool or quoted heredoc, validate against the current form, and use:

```bash
gh issue create --repo themadorg/madmail \
  --title '[Bug]: Concrete symptom' --label bug --type Bug \
  --body-file /absolute/path/to/bug-issue.md
```

Check installed `gh issue create --help` and `gh api repos/themadorg/madmail/issue-types` for type support. Never silently drop requested metadata or create missing repository labels. Verify the result with `gh issue view URL --json title,body,labels,url` and, for type, `gh api repos/themadorg/madmail/issues/NUMBER --jq .type`. Return the issue link. After an ambiguous failure inspect the returned URL and search the exact title before retrying; stop if creation status remains unknown.

For requested additional issue labels or fields, use the neighboring `madmail-issue-tags` skill. Do not infer project membership, priority, effort, or dates solely from a screenshot example.
