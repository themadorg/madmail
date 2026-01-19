# Contributing To Madmail

Thank you for your interest in contributing to Madmail! We welcome contributions from everyone.

## Quick Start for Developers

1. **Clone the repository**:
   ```bash
   git clone https://github.com/themadorg/madmail.git
   cd madmail
   ```

2. **Setup environment**:
   - Ensure you have **Go** (1.21+) installed.
   - For python tools (signing, testing, notes), install **uv**.

3. **Build the server**:
   ```bash
   make build
   ```

4. **Run tests**:
   - **Unit tests**: `make test-unit`
   - **E2E tests** (Requires `uv` and `deltachat-rpc-server`): `make test`

## Key Rules

- **Privacy First**: Madmail is a privacy-focused server. Avoid any changes that introduce unnecessary logging of user data.
- **Security**: All releases are digitally signed. If you are adding core functionality, ensure it doesn't break the upgrade mechanism. For reporting vulnerabilities, see our [Security Policy](.github/SECURITY.md).
- **AI Responsibility**: If you use AI to assist in coding, you MUST manually audit every line. You must fully understand how the code works and be able to explain it. Blindly committing AI-generated code is not allowed. See [AI Disclosure](docs/ai-disclosure.md).
- **Testing**: New features or bug fixes MUST include a corresponding test case. See [E2E Testing Guide](docs/chatmail/e2e_test.md).
- **Commit Messages**: We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification (e.g., `feat: add encryption`, `fix: handle null pointer`).
- **Code Style**: Run `make lint` before submitting a Pull Request.

## How to Contribute

1. **Check Project Priorities**: Visit our [Project Board](https://github.com/orgs/themadorg/projects/1) to see what we are currently focused on.
2. **Open an Issue**: Before starting any work, open an **Issue** to discuss your idea. Get approval from the authors first.
3. **Branching Strategy**:
   - Use `feat/branch-name` for new features.
   - Use `fix/branch-name` for bug fixes.
4. **Fork and Commit**: Fork the repository, create your branch, and commit your changes. Make sure tests pass!
5. **Submit a Pull Request**: Submit a PR once your work is ready.

For more detailed information on our development workflow, coding standards, and project architecture, please refer to the **[Detailed Contribution Guide](docs/contributing.md)**.
