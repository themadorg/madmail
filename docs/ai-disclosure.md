# AI Disclosure & Security Model

Parts of the Madmail project have been developed with the assistance of Artificial Intelligence. We believe in complete transparency regarding our development process and how it relates to the security of your data.

## 🤖 AI Involvement
Using AI to assist in coding allows for rapid feature iteration and optimization. However, it is important to clarify that this does not compromise the inherent security of the Chatmail model. AI-generated or assisted code is primarily focused on the server-side logic and orchestration, while the core encryption remains end-to-end.

## 🛡 Human Oversight & E2E Testing
To ensure that AI-assisted changes do not introduce regressions or damage existing functionality, we implement a strict **"Review & Test"** policy. Every change—whether human-written or AI-assisted—must be manually audited and passed through our comprehensive test suite.

### Automated Verification
We use a specialized End-to-End (E2E) testing framework that simulates real Delta Chat client behavior. You can run the entire suite using:
```bash
make test
```

The tests (located in [`tests/deltachat-test`](../tests/deltachat-test/)) cover critical security and functional scenarios, including:
- **[Account Creation](../tests/deltachat-test/scenarios/test_01_account_creation.py)**: Verifies JIT provisioning.
- **[Encryption Enforcement](../tests/deltachat-test/scenarios/test_02_unencrypted_rejection.py)**: Ensures the server rejects unencrypted plain-text mail.
- **[Secure Join](../tests/deltachat-test/scenarios/test_03_secure_join.py)**: Tests the specialized handshake for verified groups.
- **[Group Messaging](../tests/deltachat-test/scenarios/test_05_group_message.py)**: Verifies delivery to multiple recipients.
- **[File Transfers](../tests/deltachat-test/scenarios/test_06_file_transfer.py)** & **[Large Files](../tests/deltachat-test/scenarios/test_09_send_bigfile.py)**: Stress tests the SMTP/IMAP pipeline with binary data.
- **[Federation](../tests/deltachat-test/scenarios/test_07_federation.py)**: Ensures cross-server communication works securely.
- **[No-Log Verification](../tests/deltachat-test/scenarios/test_08_no_logging.py)**: Confirms that privacy settings are respected.
- **[Binary Signature & Upgrade](../tests/deltachat-test/scenarios/test_10_upgrade_mechanism.py)**: Verifies the security and integrity of the auto-upgrade system.

### Contributor Responsibility
Any contributor adding new features or modifying the core should add a corresponding test scenario in [`tests/deltachat-test/scenarios/`](../tests/deltachat-test/scenarios/). This collective auditing ensures that Madmail remains robust before every release.

## 🛡 Security & Privacy Model
When using Madmail with Delta Chat, it is crucial to understand what is protected and what the server's role is:

1.  **End-to-End Encryption (E2EE)**: Chatmail servers are merely temporary relays for encrypted messages. The server cannot read the content of your messages because it does not have your private keys.
2.  **Unencrypted Messages**: Delta Chat explicitly identifies unencrypted messages. If a message is not secure, it is marked with a **gray avatar** and a clear warning. This ensures you always know the status of your privacy.
3.  **Threat Model**: If a Chatmail server is compromised, the primary risk is **metadata leakage**. An attacker might discover who is communicating with whom, when, and from which IP addresses. However, your message contents remain cryptographically secure.
4.  **Trust but Verify**: While AI-assisted development provides a powerful toolset, we encourage a cautious approach. The entire codebase is open-source, allowing anyone to audit the logic and ensure there are no simple bugs or oversights that might have been missed by human maintainers.

## ⚠️ Why This Disclosure?
We include this notice to stay accountable to our users. While we thoroughly review all changes, AI can sometimes introduce "simple" logic bugs that differ from typical human errors. By providing the source code and this disclosure, we empower the community to help us maintain a robust and secure mail delivery system.

Your security is founded on **math and open-source transparency**, not just the server's configuration.
