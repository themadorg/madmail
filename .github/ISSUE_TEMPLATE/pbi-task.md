---
name: Task Template
about: Standard template for technical tasks, features, and improvements.
title: "[TASK] "
labels: task
assignees: ''

---

# 📋 Overview
<!-- Provide a 1-2 sentence summary of the goal. -->
<!-- IMPORTANT: Please apply relevant labels: 'task', 'frontend', 'backend', 'ui/ux', 'security', 'refactor' -->

## 👤 User Story / Motivation
- **As a:** [Type of user - e.g., Server Admin, Delta Chat User]
- **I want to:** [The action or feature]
- **So that:** [The benefit or value created]

---

## ✅ Acceptance Criteria (AC)
- [ ] [Criterion 1: e.g., CLI command 'maddy upgrade' supports the --force flag]
- [ ] [Criterion 2: e.g., Documentation updated in docs/upgrading.md following the Style Guide]
- [ ] [Criterion 3: e.g., A new E2E test scenario covers the change]

---

## 🛠 Implementation Strategy
<!-- Describe the technical approach, files to be modified, and new logic. -->
- **Proposed Logic:** ...
- **Affected Components:** `internal/...`, `cmd/...`
- **Privacy Considerations:** [Does this affect data retention or logging? See docs/chatmail/nolog.md]

---

## 🧪 Verification Plan
<!-- Documentation link: [E2E Testing Guide](docs/chatmail/e2e_test.md) -->
- [ ] **Unit Tests:** Run `make test-unit` and ensure core logic is covered.
- [ ] **E2E Tests:** Create/Update a test file in `tests/deltachat-test/scenarios/`.
- [ ] **Lint & Vet:** `make lint` and `make vet` MUST pass without errors.

---

## 📅 Roadmap/Checklist
- [ ] Manual review and audit of AI-assisted code (if any).
- [ ] [Conventional Commits](https://www.conventionalcommits.org/) used.
- [ ] Privacy by Design: Minimized data retention, no sensitive info in logs.
- [ ] Definition of Done (DoD) reached.
