# Detailed Contribution Guide

This document provides technical details for developers who want to dive deeper into Madmail development.

## Project Structure

- `cmd/maddy`: The main entry point for the server.
- `internal/`: Core logic, including IMAP/SMTP endpoints, authentication, and CLI tools.
- `framework/`: Reusable components and interfaces used by maddy.
- `tests/deltachat-test/`: End-to-end tests using Delta Chat RPC.
- `docs/`: Documentation (MkDocs based).

## Development Workflow

### Building
We use a `Makefile` to simplify common tasks.
- `make build`: Compiles the binary to `build/maddy`.
- `make install`: Builds and installs maddy to your local system (for testing).

### Testing
Testing is critical for Madmail. We follow a "test-first" approach for new features.
- **Go Unit Tests**: Located alongside source files. Run with `make test-unit`.
- **E2E Tests**: Python-based tests that simulate real Delta Chat clients.
    - These tests are located in `tests/deltachat-test/`.
    - Every new feature or significant bug fix **must include a new test file** in `tests/deltachat-test/scenarios/`.
    - See the [E2E Test Suite Documentation](chatmail/e2e_test.md) for details on how to write these tests.

## AI Auditing & Responsibility

We acknowledge that AI tools (like GitHub Copilot, ChatGPT, or Gemini) are powerful coding assistants. However, contributors are fully responsible for the code they submit.

1. **Mandatory Review**: You must manually review and audit any code generated or assisted by AI.
2. **Deep Understanding**: Do not submit code you don't understand. You should be able to explain the logic and flow of your contribution during the review process.
3. **Transparency**: If a significant portion of your PR was AI-assisted, please mention it.
4. **Security Audit**: AI can sometimes suggest patterns that are insecure or privacy-invasive. Always double-check against our [Security & Disclosure model](ai-disclosure.md).

## Coding Standards
- Use **Conventional Commits** for commit messages (e.g., `feat:`, `fix:`, `docs:`, `chore:`).
- Use `gofmt` for formatting.
- Follow the "Privacy by Design" principle: minimize data retention.
- All new CLI commands should be implemented in `internal/cli/ctl/` following existing patterns.

## Release Signing and Upgrades
Madmail uses **Ed25519** signatures for binary verification.
- The public key is hardcoded in `internal/auth/signature_key.go`.
- The private key is kept outside the repository.
- If you modify the `upgrade` or `update` commands, you **must** verify them using `make test --test-10`.

## Submission Process

1. **Check Priorities**: Review the [Project Board](https://github.com/orgs/themadorg/projects/1) for current priorities.
2. **Open an Issue**: Use the [Issue Template](https://github.com/themadorg/madmail/issues/new/choose) to describe your proposal. **Wait for approval** from the main authors before writing code.
3. **Branching**:
   - `feat/feature-short-name` for new features.
   - `fix/bug-fix-short-name` for bug fixes.
4. **Lint & Test**: Ensure your code is linted (`make lint`) and passes all tests (`make test`).
5. **PR**: Keep PRs focused. Fill out the [Pull Request Template](../.github/PULL_REQUEST_TEMPLATE.md) completely.

## Getting Help
If you have questions, feel free to:
- Open a GitHub Issue.
- Join our community discussions.
- Email the maintainers.
