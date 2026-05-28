# Testing WebIMAP in Core (before desktop)

## Quick commands

From **madmailv2** (builds `chatmail`, runs Core tests):

```bash
chmod +x scripts/core-e2e-webimap.sh
./scripts/core-e2e-webimap.sh
```

From **desktop/core** directly:

```bash
# Unit only (no server)
cargo test p1_ut

# Local chatmail subprocess
export CHATMAIL_BIN=/path/to/madmailv2/target/debug/chatmail
export CHATMAIL_WEBIMAP_TEST=1
cargo test webtransport_local -- --nocapture

# Remote server (set host and credentials via env — do not commit real values)
export WEBIMAP_REMOTE_TEST=1
export WEBIMAP_TEST_BASE_URL=      # required, e.g. https://YOUR_HOST
export WEBIMAP_TEST_EMAIL=         # required
export WEBIMAP_TEST_PASSWORD=      # required
./scripts/core-e2e-webimap.sh remote
```

## What each tier checks

| Tier | Env | Tests |
|------|-----|--------|
| Unit | — | `p1_ut00` … `p1_ut02` in `webtransport::*` |
| Local IT | `CHATMAIL_WEBIMAP_TEST=1` | spawn chatmail, enable webimap/websmtp via `POST /api/admin`, Core `probe_webimap`, REST list, WS `list_mailboxes` |
| Remote | `WEBIMAP_REMOTE_TEST=1` + base URL + creds | Core probe + WS against live host |

## Test files

- `desktop/core/src/webtransport/` — implementation
- `desktop/core/src/tests/chatmail_webtransport.rs` — integration tests
- `desktop/core/src/tests/chatmail_transport.rs` — HTTP/SMTP helpers + `spawn_chatmail_with_webimap`

## Desktop

Only after local + remote Core tests pass:

1. Rebuild RPC server: `cd desktop/core/deltachat-rpc-server/npm-package && python scripts/make_local_dev_version.py`
2. Restart Electron
3. Enable **WebIMAP transport** in Advanced (chatmail account)
4. Set `webimap_base_url` if needed
