# Context & reference projects

Madmail v2 is developed alongside a `**context/**` directory: full git checkouts (or symlinks to them) of related mail, Delta Chat, and infrastructure projects. They are **reference material** — most of this code is **not** compiled into the shipping `chatmail` binary.

A second tree, `**external/`**, holds editable submodules (e.g. the admin web UI) whose build output **is** embedded in releases.

---

## How these projects were used

> **Placeholder** — to be filled in with a concrete description of how each tree was used during development (parity reading, test harnesses, protocol study, deployment layout, etc.).

At a high level:

- `**context/`** — read-only archaeology: “what did Madmail v1 do?”, “how does Stalwart structure SMTP in Rust?”, “what does a real Delta Chat client send?”
- `**external/**` — co-developed frontend and other assets shipped inside the binary.

For developer-oriented notes, see also:

- [14 — Understanding `context/` and `external/](project/14-understanding-context-and-references.md)`
- [TDD — Implementation reference codebases](TDD/CONTEXT.md)

---

## `context/` — reference trees


| Path                                                          | Project                         | Git repository                                                                | License           | Role                                                                                               |
| ------------------------------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------- | ----------------- | -------------------------------------------------------------------------------------------------- |
| `[context/madmail/](../context/madmail/)`                     | **Madmail v1** (Go)             | [themadorg/madmail](https://github.com/themadorg/madmail)                     | GPL-3.0           | Primary **behavior reference** — SMTP, IMAP, `/mxdeliv`, admin API, PGP, JIT, quota, operator docs |
| `[context/stalwart/](../context/stalwart/)`                   | **Stalwart Mail Server**        | [stalwartlabs/stalwart](https://github.com/stalwartlabs/stalwart)             | AGPL-3.0-only     | Modern Rust **protocol engine** study (SMTP/IMAP session structure, parsers)                       |
| `[context/cmrelay/](../context/cmrelay/)`                     | **cmrelay** (Classic Mad Relay) | —                                                                             | —                 | Legacy Dovecot/Postfix-era stack — federation, metadata, JIT hooks, installer layout               |
| `[context/cmdeploy/](../context/cmdeploy/)`                   | **cmdeploy**                    | [chatmail/cmdeploy](https://github.com/chatmail/cmdeploy)                     | MIT               | Deployment templates (Postfix, Dovecot, nginx) and online pytest spec                              |
| `[context/core/](../context/core/)`                           | **Delta Chat Core**             | [chatmail/core](https://github.com/chatmail/core)                             | MPL-2.0           | Real client library for E2E tests (`make test-deltachat`)                                          |
| `[context/deltachat-desktop/](../context/deltachat-desktop/)` | **Delta Chat Desktop**          | [deltachat/deltachat-desktop](https://github.com/deltachat/deltachat-desktop) | GPL-3.0           | Desktop client reference                                                                           |
| `[context/calls-webapp/](../context/calls-webapp/)`           | **calls-webapp**                | [deltachat/calls-webapp](https://github.com/deltachat/calls-webapp)           | GPL-3.0           | WebRTC calls UI reference                                                                          |
| `[context/turn-rs/](../context/turn-rs/)`                     | **turn-rs**                     | [mycrl/turn-rs](https://github.com/mycrl/turn-rs)                             | MIT               | TURN/STUN server reference — embedded in `chatmail-turn`                                           |
| `[context/chatmail-turn/](../context/chatmail-turn/)`         | **chatmail-turn**               | [chatmail/chatmail-turn](https://github.com/chatmail/chatmail-turn)           | ISC               | Chatmail TURN integration reference                                                                |
| `[context/iroh/](../context/iroh/)`                           | **Iroh**                        | [n0-computer/iroh](https://github.com/n0-computer/iroh)                       | Apache-2.0 OR MIT | P2P / relay stack — `chatmail-iroh` supervises compatible `iroh-relay`                             |
| `[context/webrtc/](../context/webrtc/)`                       | **webrtc-rs**                   | [webrtc-rs/webrtc](https://github.com/webrtc-rs/webrtc)                       | MIT OR Apache-2.0 | WebRTC types and patterns for calls                                                                |
| `[context/rtc/](../context/rtc/)`                             | **rtc**                         | [webrtc-rs/rtc](https://github.com/webrtc-rs/rtc)                             | MIT OR Apache-2.0 | RTC protocol building blocks                                                                       |
| `[context/cmlxc/](../context/cmlxc/)`                         | **cmlxc**                       | [chatmail/cmlxc](https://github.com/chatmail/cmlxc)                           | MPL-2.0           | Disposable VM harness for integration / E2E                                                        |
| `[context/cmping/](../context/cmping/)`                       | **cmping**                      | [chatmail/cmping](https://github.com/chatmail/cmping)                         | MPL-2.0           | Connectivity / ping tooling                                                                        |
| `[context/relay-ping/](../context/relay-ping/)`               | **relay-ping**                  | [themadorg/relay-ping](https://github.com/themadorg/relay-ping)               | ISC               | Step-by-step relay test tool (`dclogin` workflows)                                                 |
| `[context/certbot/](../context/certbot/)`                     | **Certbot**                     | [certbot/certbot](https://github.com/certbot/certbot)                         | Apache-2.0        | ACME / Let's Encrypt reference                                                                     |
| `[context/lers/](../context/lers/)`                           | **lers**                        | [akrantz01/lers](https://github.com/akrantz01/lers)                           | MIT               | ACME client reference (HTTP-01)                                                                    |
| `[context/data/](../context/data/)`                           | *(local)*                       | —                                                                             | —                 | Local test fixtures and sample data (not a git repo)                                               |


**Stalwart checkout:** Before Madmail v2 development started, the `context/stalwart/` tree was trimmed so that **all non–open-source components were removed**, leaving only material we could study and reference under open licenses (see AGPL-3.0-only above).

> **Note:** Some `context/` entries are **symlinks** to sibling checkouts on a developer machine. Clone or link the repositories above into `context/<name>/` as needed. Exact remotes on your machine may differ if you use a fork. Licenses are taken from each tree’s `LICENSE` / `COPYING` / `Cargo.toml` / `package.json` where present; **cmrelay** was not audited here.

---

## `external/` — shipped submodules


| Path                                                            | Project               | Git repository                                                                | License | Role                                                                                             |
| --------------------------------------------------------------- | --------------------- | ----------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------ |
| `[external/madmail-admin-web/](../external/madmail-admin-web/)` | **Madmail Admin Web** | [themadorg/madmail-admin-web](https://github.com/themadorg/madmail-admin-web) | GPL-3.0 | SvelteKit admin dashboard — built and **embedded** into the binary (`make build-with-admin-web`) |


See `[external/README.md](../external/README.md)` for submodule setup.

---

## Contributing to this list

When you document **how** a tree was used, add a subsection under [How these projects were used](#how-these-projects-were-used) above — per project or grouped by theme (parity, protocols, calls, TLS, testing).