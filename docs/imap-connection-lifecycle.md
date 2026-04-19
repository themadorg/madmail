# IMAP connection lifecycle: goroutines and memory

This note describes **where per-connection work runs in Go** and **what runs when an IMAP session ends** (clean `LOGOUT`, TCP reset, EOF, or read error).

**Relevant paths in this repository**

| Role | Path |
|------|------|
| IMAP endpoint, listener goroutines, `Serve` wiring | `internal/endpoint/imap/imap.go` |
| Selected mailbox cleanup (`Mailbox.Close` → mess handle) | `internal/go-imap-sql/mailbox.go` |
| Per-session `User.Logout()` (no-op here) | `internal/go-imap-sql/user.go` |

**Dependency (not in-tree):** `go.mod` replaces `github.com/emersion/go-imap` with **`github.com/foxcpp/go-imap`**. Session logic lives under that module, e.g. **`server/server.go`** (`Serve`, `serveConn`) and **`server/conn.go`** (`conn.serve`, `conn.Close`, `setDeadline`, `send`).

---

## 1. Goroutines in this repo (listener)

For each bound address, the IMAP endpoint starts **one long-lived goroutine** that calls `imapserver.Server.Serve` on that listener. That goroutine only exits when accept fails (for example listener closed on shutdown).

**File:** `internal/endpoint/imap/imap.go`

```211:217:internal/endpoint/imap/imap.go
		endp.listenersWg.Add(1)
		go func() {
			if err := endp.serv.Serve(l); err != nil && !strings.HasSuffix(err.Error(), "use of closed network connection") {
				endp.Log.Printf("imap: failed to serve %s: %s", addr, err)
			}
			endp.listenersWg.Done()
		}()
```

This is **not** the per-connection worker; it is the accept loop for that listener.

---

## 2. Goroutines in the IMAP server library (per client)

Inside **`github.com/foxcpp/go-imap/server`** (`Server.Serve` in **`server/server.go`**), each accepted TCP connection gets:

1. **`go s.serveConn(conn)`** — main handler for that client: registers the connection, runs the command loop, and on exit **always** runs deferred cleanup (see §3).
2. **`go conn.send()`** — started from **`newConn`** in **`server/conn.go`**; pushes bytes to the client. It stops when **`conn.serve`** closes the **`loggedOut`** channel (so the send loop does not outlive the session handler).

Registration and cleanup of the active connection set use the server’s **`conns`** map (**`server/server.go`**): on shutdown of a session, **`delete(s.conns, conn)`** removes that connection from the server’s bookkeeping so it is no longer referenced there.

---

## 3. What happens when the connection “dies”

Typical paths:

- Client sends **`LOGOUT`** → handler sets state to logout → read loop exits (**`server/cmd_any.go`**, `Logout` handler).
- Client closes TCP → **`ReadLine`** gets **`io.EOF`** → **`conn.serve`** returns (**`server/conn.go`**).
- Network error → read/write failure → **`serve`** returns with an error.

Then (**`server/server.go`**, **`serveConn`** defer; **`server/conn.go`**, **`serve`** defers):

1. **`conn.serve` defer** runs: sets logout state and **`close(loggedOut)`**, which unblocks the **`send`** goroutine so it exits.
2. **`serveConn` defer** runs: **`conn.Close()`**, then **`delete(s.conns, conn)`**.

So the **per-connection goroutine** (`serveConn`) finishes; the **`send`** goroutine finishes; the connection object is dropped from the server map. Remaining heap objects become unreachable and are reclaimed by the **GC** like any other Go value (there is no separate “free” step for goroutine stacks beyond the runtime reclaiming them when the goroutine exits).

### When the peer is gone but nothing “signals” the server yet

Sometimes the remote end disappears **without** a clean TCP teardown: no **`FIN`**, no **`RST`**, no bytes on the wire (laptop sleep, pulled cable on client side only, routing black hole, etc.). From the server’s point of view the socket can stay **half-open**: the kernel may still report the connection as **established**, and the IMAP handler stays blocked in **`ReadLine`** waiting for the next command.

In that situation:

- **No immediate cleanup runs.** The **`serveConn`** and **`send`** goroutines **stay alive**, and the session **remains** in **`Server.conns`** until something makes **`Read`** or **`Write`** fail or return **`EOF`**.
- **Application idle timeout:** In **`server/conn.go`**, **`setDeadline()`** only applies read/write deadlines when **`Server.AutoLogout`** is non-zero (and not below **`MinAutoLogout`**). **This codebase does not set `AutoLogout`** on the IMAP server (see `internal/endpoint/imap/imap.go`: only `New`, `Init`, listeners, etc.), so the library **does not** impose an IMAP-layer idle deadline by default.
- **Eventual detection** is then up to **TCP behaviour** (retransmits, optional **TCP keepalive** at the OS/socket level, middlebox timeouts) or **operator action** (closing the listener / process, firewall killing idle flows). Until one of those fires, there is **no “connection gone” event** and **no** **`conn.Close()`** / mailbox teardown for that session.

So: **“no signal whatsoever” means the Go side can wait a long time (or indefinitely, depending on OS defaults), and memory and goroutines for that client are not released until the stack finally surfaces an error or EOF on the socket.**

---

## 4. What `conn.Close()` does (fork vs upstream)

In the **foxcpp `go-imap` fork** used by this project (**`server/conn.go`**, **`Close()`**), **`Close()` closes a selected mailbox before logging the user out**, then closes the TCP connection:

- If **`ctx.Mailbox != nil`** → **`ctx.Mailbox.Close()`**  
  For storage here, that ends up calling **`Mailbox.Close()`** in **`internal/go-imap-sql/mailbox.go`**, which closes the **`go-imap-mess` `MailboxHandle`** (releases manager state for that session).
- If **`ctx.User != nil`** → **`ctx.User.Logout()`**.

In **this** codebase, **`User.Logout()`** for the SQL backend is currently a no-op (**`internal/go-imap-sql/user.go`**):

```344:346:internal/go-imap-sql/user.go
func (u *User) Logout() error {
	return nil
}
```

So **session teardown that matters for shared mailbox state is driven by `Mailbox.Close()`** from the fork’s **`conn.Close()`**, not by **`User.Logout()`** in **`internal/go-imap-sql`**.

---

## 5. Shutdown of the whole IMAP endpoint

Closing listeners and the server waits for listener goroutines; see **`Endpoint.Close()`** in **`internal/endpoint/imap/imap.go`** (**`listenersWg.Wait()`** after **`serv.Close()`**).

---

## 6. Implemented test coverage

The test plan above is now implemented in-tree as:

- `tests/imap_connection_lifecycle_test.go`

It validates two scenarios:

1. **Normal disconnect** (open session, then `LOGOUT`, cleanup expected quickly).
2. **Held-open and optional silent-drop** (connection kept open; optional packet blackhole mode to emulate "no signal" behavior).

The test prints and checks:

- goroutine counters (`runtime.NumGoroutine`) for debug trend visibility
- established TCP connection counters for the IMAP port (from `/proc/net/tcp` and `/proc/net/tcp6`)

Run command (build **maddy** with `-tags debugflags` first so `-debug.smtpport` exists; pass the binary explicitly because `go test` runs from package `tests/` and `./maddy` there is wrong):

```bash
go build -tags 'debugflags,cgo,!nosqlite3' -o maddy ./cmd/maddy
go test -tags 'integration,cgo,!nosqlite3' ./tests \
  -integration.executable="$(pwd)/maddy" \
  -run TestIMAPConnectionLifecycle_DebugCounters -count=1 -v
```

Optional "no signal" phase (requires root + iptables):

```bash
sudo MADDY_TEST_SILENT_DROP=1 go test -tags 'integration,cgo,!nosqlite3' ./tests \
  -integration.executable="$(pwd)/maddy" \
  -run TestIMAPConnectionLifecycle_DebugCounters -count=1 -v
```

To compare **`auto_logout`** off vs **1m** against live **pprof** goroutine totals (same workflow as above, uses **`tests/test-client`** for IMAP idle):

```bash
./scripts/goroutine_idle_experiment.sh
```

## 7. Test harness reliability fixes

To make the lifecycle test stable, the integration harness was hardened in:

- `tests/t.go`

Changes made:

- DNS server startup now retries transient bind conflicts (`address already in use`).
- test port allocation now reserves real ephemeral ports via kernel (`127.0.0.1:0`) instead of random-only selection.
- random generator is seeded in `init()` to reduce repeated collision patterns across processes.

These changes fix the immediate setup failure seen during test execution (mockdns bind conflict).

## 8. What is fixed vs not fixed

### Fixed

- The reproducible test workflow is implemented and automated in Go integration tests.
- The "address already in use" test-setup flake is fixed in the harness.

### Fixed (product behavior)

- IMAP endpoint now configures server-side idle deadline via `auto_logout` in `internal/endpoint/imap/imap.go`.
- Default value is `30m`, and it is wired to `endp.serv.AutoLogout`.
- This means half-open sessions will be closed by deadline instead of potentially living forever with no signal.

Configuration example:

```conf
imap tcp://127.0.0.1:143 {
    auto_logout 30m
    # ... other settings ...
}
```

---

**Takeaway:** The **Go “task” per IMAP client** is the library’s **`serveConn` goroutine** (plus the paired **`send`** goroutine). When the session ends normally, **`serveConn`’s deferred `conn.Close()`** runs (mailbox close on the foxcpp fork, then **`User.Logout()`**, then TCP close), then the connection is **removed from `Server.conns`**. After that, memory is reclaimed like normal Go objects; **`internal/go-imap-sql`** does not add extra teardown in **`Logout()`** beyond what **`Mailbox.Close()`** already did. **With `auto_logout` now set in IMAP endpoint config (default `30m`), idle/half-open sessions are also forced to close by deadline.**
