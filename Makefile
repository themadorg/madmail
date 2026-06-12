# madmail-v2 — developer targets (adapted from context/madmail/Makefile)
#
# Madmail (Go/maddy) originals: build.sh, systemd install, sign/deploy.
# Deploy: `make push` builds release chatmail, scp's to test servers, replaces binary, restarts systemd (no signing).

.PHONY: all init build build-admin-web build-chatmail-embed build-chatmail-embed-release build-with-admin-web build-release build-profiling build-release-static build-workspace build-all build-landing preview-landing \
	test test-unit test-integration test-e2e test-maintenance test-imap test-turn test-core-turn test-deltachat test-dclogin relay-ping-build relay-ping-clean t1-bench t1-report-demo \
	check vet lint fmt fmt-check run run-bg run-debug restart stop logs reset-db dev-certs clean help \
	sign push push1 push2 log1 log2 push-signed publish init-publish build-publish \
	man man-lint man-check

# Optional overrides (copy .env.example → .env; publish merges context/madmail/.env into .env)
-include .env
export

# ── Paths & binaries ─────────────────────────────────────────────────────────
BINARY_DEBUG     := target/debug/madmail
BINARY_RELEASE   := target/release/madmail
BINARY_PROFILING := build/madmail-linux-amd64-profiling
BINARY_PUSH      ?= $(BINARY_RELEASE)
STATE_DIR        ?= ./data
CONFIG           ?= $(STATE_DIR)/chatmail.toml
CHATMAIL_FLAGS   := --state-dir $(STATE_DIR) --config $(CONFIG)
LOG_FILE         ?= /tmp/chatmail.log
PID_FILE         ?= /tmp/chatmail.pid

RELAY_PING_DIR   := context/relay-ping
RELAY_PING_BIN   := $(RELAY_PING_DIR)/bin/relay-ping

# relay-ping / dclogin (override in .env when testing two accounts)
DCLOGIN1         ?=
DCLOGIN2         ?=
RELAY_TIMEOUT    ?= 3m
RELAY_STEP_TIMEOUT ?= 45s

# Remote deploy hosts (paths/service defaults live in scripts/deploy.sh)
REMOTE1          ?=
REMOTE2          ?=
# Madmail admin-web submodule (external/madmail-admin-web). Override in .env if needed.
ADMIN_WEB_DIR    ?= external/madmail-admin-web
ADMIN_WEB_BUILD  := $(ADMIN_WEB_DIR)/build
LANDING_DIR      ?= landing

# scripts/publish.sh flags (e.g. --no-github-release). Not the `init` target — use `make init publish`.
PUBLISH_ARGS ?=
# Legacy name; `init` is stripped (asset setup is `make init`, then `make publish`).
_publish_args := $(strip $(filter-out init,$(PUBLISH_ARGS) $(ARGS)))

# iroh-relay v0.35.0 (musl) for chatmail-iroh embed (cmdeploy / Delta Chat parity)
IROH_RELAY_VERSION ?= v0.35.0
IROH_ASSETS        := crates/chatmail-iroh/assets
IROH_BINARY        := $(IROH_ASSETS)/iroh-relay

# ── Default ──────────────────────────────────────────────────────────────────
all: build

# ── Init (first-time dev assets) ─────────────────────────────────────────────
# Download iroh-relay into $(IROH_ASSETS)/ (skipped if already present). Override: IROH_ARCH, IROH_RELAY_VERSION.
init:
	@bash -euo pipefail -c '\
	assets="$(abspath $(IROH_ASSETS))"; \
	binary="$$assets/iroh-relay"; \
	version="$(IROH_RELAY_VERSION)"; \
	if [ -f "$$binary" ]; then \
		echo "iroh-relay already present: $$binary"; \
		exit 0; \
	fi; \
	if [ -n "$${IROH_ARCH:-}" ]; then \
		iroh_arch="$$IROH_ARCH"; \
	else \
		case "$$(uname -m)" in \
			x86_64) iroh_arch=x86_64-unknown-linux-musl ;; \
			aarch64|arm64) iroh_arch=aarch64-unknown-linux-musl ;; \
			*) echo "Unsupported host arch: $$(uname -m) (set IROH_ARCH explicitly)" >&2; exit 1 ;; \
		esac; \
	fi; \
	command -v curl >/dev/null || { echo "curl required" >&2; exit 1; }; \
	command -v tar >/dev/null || { echo "tar required" >&2; exit 1; }; \
	mkdir -p "$$assets"; \
	url="https://github.com/n0-computer/iroh/releases/download/$${version}/iroh-relay-$${version}-$${iroh_arch}.tar.gz"; \
	tarball="$$assets/iroh-relay.tar.gz"; \
	echo "-- Downloading iroh-relay $${version} ($${iroh_arch})..."; \
	curl -fsSL "$$url" -o "$$tarball"; \
	tar -xzf "$$tarball" -C "$$assets"; \
	rm -f "$$tarball"; \
	if [ ! -f "$$binary" ]; then \
		while IFS= read -r -d "" p; do \
			if [ "$$(basename "$$p")" = iroh-relay ]; then mv "$$p" "$$binary"; break; fi; \
		done < <(find "$$assets" -type f -name iroh-relay -print0 2>/dev/null); \
	fi; \
	if [ ! -f "$$binary" ]; then echo "iroh-relay binary not found after extract" >&2; exit 1; fi; \
	chmod +x "$$binary"; \
	printf "%s\n" "$$version" >"$$assets/VERSION"; \
	echo "Installed $$binary ($$(wc -c <"$$binary" | tr -d " ") bytes)"; \
	'

# ── Build ────────────────────────────────────────────────────────────────────
# Madmail (context/madmail/build.sh `copy_admin_web`) on every `make build`:
#   1. (bun|npm) run build  → admin-web/build/
#   2. stamp build/version.json (service worker cache bust)
#   3. cp -r admin-web/build → internal/adminweb/build → go:embed in maddy
# madmail-v2: build.rs copies $(ADMIN_WEB_BUILD) → crates/chatmail-admin-web/embed/
# when CHATMAIL_ADMIN_WEB_BUILD is set (see build-chatmail-embed* targets).
# After changing admin-web/src: `make build-with-admin-web` then `make restart`.
# `make build` / `make restart` alone do NOT rebuild or re-embed the SPA.
# npm build + stamp version.json + cargo (re-embed SPA into chatmail binary)
build-admin-web:
	@if [ ! -f "$(ADMIN_WEB_DIR)/package.json" ]; then \
		echo "-- Initializing admin-web submodule ($(ADMIN_WEB_DIR))..."; \
		git submodule update --init $(ADMIN_WEB_DIR); \
	fi
	@if [ -f "$(ADMIN_WEB_DIR)/package.json" ]; then \
		if command -v bun >/dev/null 2>&1; then \
			echo "-- Building admin-web from $(ADMIN_WEB_DIR) (bun)..."; \
			cd $(ADMIN_WEB_DIR) && bun install && bun run build; \
		elif command -v npm >/dev/null 2>&1; then \
			echo "-- Building admin-web from $(ADMIN_WEB_DIR) (npm)..."; \
			cd $(ADMIN_WEB_DIR) && npm install && npm run build; \
		else \
			echo "-- [!] No bun or npm found."; exit 1; \
		fi; \
	else \
		echo "-- [!] $(ADMIN_WEB_DIR)/package.json missing (run: git submodule update --init $(ADMIN_WEB_DIR))"; exit 1; \
	fi
	@test -f "$(ADMIN_WEB_BUILD)/index.html" || (echo "-- [!] $(ADMIN_WEB_BUILD)/index.html missing after SPA build"; exit 1)
	@VER=$$(cat $(ADMIN_WEB_DIR)/.version 2>/dev/null \
		|| node -p "require('./$(ADMIN_WEB_DIR)/package.json').version" 2>/dev/null \
		|| echo dev); \
		echo "{\"version\":\"$$VER\"}" > $(ADMIN_WEB_BUILD)/version.json; \
		echo "-- Stamped $(ADMIN_WEB_BUILD)/version.json with admin-web $$VER"

# Embed $(ADMIN_WEB_BUILD) into chatmail-admin-web/embed/ at compile time.
build-chatmail-embed:
	@test -f "$(ADMIN_WEB_BUILD)/index.html" || (echo "-- [!] Missing $(ADMIN_WEB_BUILD); run make build-admin-web first"; exit 1)
	CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" cargo build -p chatmail

build-chatmail-embed-release:
	@test -f "$(ADMIN_WEB_BUILD)/index.html" || (echo "-- [!] Missing $(ADMIN_WEB_BUILD); run make build-admin-web first"; exit 1)
	CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" cargo build -p chatmail --release

build-with-admin-web: build-admin-web build-chatmail-embed

build:
	cargo build -p chatmail

build-release: build-admin-web build-chatmail-embed-release

# Release binary with CPU profiling server on :6060 (/debug/pprof/flamegraph). For benchmarks only.
build-profiling: build-admin-web
	@test -f "$(ADMIN_WEB_BUILD)/index.html" || (echo "-- [!] Missing $(ADMIN_WEB_BUILD); run make build-admin-web first"; exit 1)
	CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" cargo build -p chatmail --release --features pprof
	@mkdir -p build
	@cp $(BINARY_RELEASE) $(BINARY_PROFILING)
	@echo "-- Profiling binary: $(BINARY_PROFILING) (pprof on http://127.0.0.1:6060/debug/pprof/)"

# Static-pie release binary (glibc + vendored OpenSSL/sqlite); runs on Debian 12+ without host glibc match.
# Uses `cargo rustc` so proc-macros are not built with +crt-static (see scripts/build-release-static.sh).
build-release-static:
	@chmod +x scripts/build-release-static.sh
	@./scripts/build-release-static.sh

build-workspace:
	cargo build --workspace

# Static SvelteKit site (landing/): docs site + marketing pages for GitHub Pages.
build-landing:
	@command -v bun >/dev/null || { echo "build-landing needs bun: https://bun.sh"; exit 1; }
	cd $(LANDING_DIR) && bun install && bun run build

preview-landing:
	@command -v bun >/dev/null || { echo "preview-landing needs bun: https://bun.sh"; exit 1; }
	cd $(LANDING_DIR) && bun run preview

# Cross-compile release binaries (requires rustup targets installed)
build-all: build-release
	@echo "Tip: install targets with:"
	@echo "  rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu"
	-cargo build -p chatmail --release --target x86_64-unknown-linux-gnu
	-cargo build -p chatmail --release --target aarch64-unknown-linux-gnu

# ── Quality ──────────────────────────────────────────────────────────────────
check:
	cargo check --workspace

vet: check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

test-unit:
	cargo test --workspace

test-integration:
	cargo test -p chatmail-integration

# In-process E2E (IMAP, SMTP, Secure Join, TURN, ctl, OpenMetrics, boot). Builds chatmail first.
test-e2e: build
	cargo test -p chatmail-integration

# Scheduled maintenance: dormant accounts, message retention, purge seen/unread.
test-maintenance:
	cargo test -p chatmail-db maintenance
	cargo test -p chatmail-storage purge
	cargo test -p chatmail-tasks

test-imap:
	cargo test -p chatmail-imap

# Phase 9 TURN: unit + smoke + integration/E2E (see docs/plans/b9/README.md)
test-turn:
	cargo test -p chatmail-turn
	cargo test -p chatmail-config p9_ut03
	cargo test -p chatmail-imap p9_ut04
	cargo test -p chatmail-integration turn_

test-core-turn:
	@test -x scripts/core-e2e-turn.sh || (echo "scripts/core-e2e-turn.sh missing (P9-S10)"; exit 1)
	./scripts/core-e2e-turn.sh

# Delta Chat RPC E2E (deltachat-test) in Incus: static binary deploy + cmlxc test runner.
# First time: make test-deltachat DC_TEST_ARGS='--init'
# Optional: CHATMAIL_BIN=target/release/madmail make test-deltachat
test-deltachat:
	chmod +x scripts/deltachat-test-incus.sh scripts/deltachat-test-deploy.py
	@command -v uv >/dev/null || (echo "test-deltachat needs uv: https://docs.astral.sh/uv/"; exit 1)
	@command -v incus >/dev/null || (echo "test-deltachat needs incus on PATH"; exit 1)
	./scripts/deltachat-test-incus.sh $(DC_TEST_ARGS)

test: test-unit

# Full SMTP/IMAP/Secure Join probe against a running local chatmail (ports 1143/2525).
# Set DCLOGIN1 and DCLOGIN2 in .env, or pass: make test-dclogin DCLOGIN1='dclogin:...' DCLOGIN2='dclogin:...'
test-dclogin: relay-ping-build
	@test -n "$(DCLOGIN1)" && test -n "$(DCLOGIN2)" || \
		(echo "Set DCLOGIN1 and DCLOGIN2 (see .env.example)"; exit 1)
	$(RELAY_PING_BIN) -test dclogin \
		-dclogin1 '$(DCLOGIN1)' -dclogin2 '$(DCLOGIN2)' \
		-log-file - -timeout $(RELAY_TIMEOUT) -step-timeout $(RELAY_STEP_TIMEOUT) -vv

# ── Run local server ─────────────────────────────────────────────────────────
run: build
	$(BINARY_DEBUG) $(CHATMAIL_FLAGS)

run-debug: build
	$(BINARY_DEBUG) $(CHATMAIL_FLAGS) --debug

# Self-signed cert for local IMAP/SMTP TLS (127.0.0.1)
dev-certs:
	@mkdir -p $(STATE_DIR)/certs
	@if [ ! -f $(STATE_DIR)/certs/fullchain.pem ]; then \
		openssl req -x509 -newkey rsa:2048 -nodes \
			-keyout $(STATE_DIR)/certs/privkey.pem \
			-out $(STATE_DIR)/certs/fullchain.pem \
			-days 3650 \
			-subj "/CN=127.0.0.1" \
			-addext "subjectAltName=IP:127.0.0.1,DNS:localhost"; \
		echo "Created dev TLS certs in $(STATE_DIR)/certs"; \
	else \
		echo "Dev TLS certs already in $(STATE_DIR)/certs"; \
	fi

# Background dev server (HTTP 8080; mail 993/143/465/587 — Madmail defaults, see `make dev-certs`)
run-bg: build
	@if [ -f $(PID_FILE) ] && kill -0 $$(cat $(PID_FILE)) 2>/dev/null; then \
		echo "madmail already running (pid $$(cat $(PID_FILE)))"; \
	else \
		nohup $(BINARY_DEBUG) $(CHATMAIL_FLAGS) --debug > $(LOG_FILE) 2>&1 & \
		echo $$! > $(PID_FILE); \
		echo "Started madmail pid $$(cat $(PID_FILE)), log: $(LOG_FILE)"; \
	fi

# Bind 25/143/465/587/993 without root (Linux, optional)
dev-bind-cap: build
	@command -v setcap >/dev/null || (echo "install libcap (setcap)"; exit 1)
	sudo setcap 'cap_net_bind_service=+ep' $(BINARY_DEBUG)
	@echo "Granted cap_net_bind_service on $(BINARY_DEBUG)"

restart: stop dev-certs run-bg

stop:
	@if [ -f $(PID_FILE) ]; then \
		kill $$(cat $(PID_FILE)) 2>/dev/null || true; \
		rm -f $(PID_FILE); \
	fi
	-pkill -f 'target/debug/madmail.*$(STATE_DIR)' 2>/dev/null || true
	@echo "Stopped madmail (if it was running)"

logs:
	@tail -f $(LOG_FILE)

# Wipe local SQLite (fixes sqlx VersionMismatch after migration .sql edits)
reset-db: stop
	@rm -f $(STATE_DIR)/chatmail.db $(STATE_DIR)/chatmail.db-wal $(STATE_DIR)/chatmail.db-shm
	@rm -f $(STATE_DIR)/credentials.db $(STATE_DIR)/credentials.db-wal $(STATE_DIR)/credentials.db-shm
	@echo "Removed local DB files in $(STATE_DIR) (mail/ and config kept)"

# Local dev with admin UI (madmail: install — rebuilds binary + restarts service)
install: build-with-admin-web restart
	@echo "Local madmail running with $(CONFIG)"

sign:
	@chmod +x scripts/sign.sh
	@./scripts/sign.sh

push: build-release
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh "$(REMOTE1)"
	@./scripts/deploy.sh "$(REMOTE2)"

push1: build-release
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh "$(REMOTE1)"

push2: build-release-static sign
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh "$(REMOTE2)" --signed

push-signed: build-release sign
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh "$(REMOTE1)" --signed
	@./scripts/deploy.sh "$(REMOTE2)" --signed

log1:
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh --log "$(REMOTE1)"

log2:
	@chmod +x scripts/deploy.sh
	@./scripts/deploy.sh --log "$(REMOTE2)"

build-publish: build-release
	@mkdir -p build
	@if [ -f target/x86_64-unknown-linux-gnu/release/madmail ]; then \
		cp target/x86_64-unknown-linux-gnu/release/madmail build/madmail-linux-amd64; \
	else \
		cp $(BINARY_RELEASE) build/madmail-linux-amd64; \
	fi
	@rustup target add aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf x86_64-pc-windows-gnu >/dev/null 2>&1 || true
	@command -v aarch64-linux-gnu-gcc >/dev/null || { echo "Install aarch64-linux-gnu-gcc for ARM64 release" >&2; exit 1; }
	@CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" \
		cargo build -p chatmail --release --target aarch64-unknown-linux-gnu
	@cp target/aarch64-unknown-linux-gnu/release/madmail build/madmail-linux-arm64
	@if command -v arm-linux-gnueabihf-gcc >/dev/null; then \
		CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" \
			cargo build -p chatmail --release --target armv7-unknown-linux-gnueabihf; \
		cp target/armv7-unknown-linux-gnueabihf/release/madmail build/madmail-linux-arm; \
	else \
		rm -f build/madmail-linux-arm; \
		echo "Skipping madmail-linux-arm (32-bit): install arm-linux-gnueabihf-gcc" >&2; \
	fi
	@command -v x86_64-w64-mingw32-gcc >/dev/null || { echo "Install mingw-w64-gcc (x86_64-w64-mingw32-gcc) for Windows release" >&2; exit 1; }
	@CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" \
		cargo build -p chatmail --release --target x86_64-pc-windows-gnu
	@cp target/x86_64-pc-windows-gnu/release/madmail.exe build/madmail-windows-amd64.exe
	@$(MAKE) build-release-static
	@cp $(BINARY_RELEASE) build/madmail-linux-amd64-legacy
	@rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl >/dev/null 2>&1 || true
	@command -v zig >/dev/null || { echo "Install zig for musl release (https://ziglang.org/download/)" >&2; exit 1; }
	@command -v cargo-zigbuild >/dev/null || { echo "Install cargo-zigbuild for musl release: cargo install cargo-zigbuild" >&2; exit 1; }
	@CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" \
		cargo zigbuild -p chatmail --release --target x86_64-unknown-linux-musl
	@cp target/x86_64-unknown-linux-musl/release/madmail build/madmail-linux-amd64-musl
	@CHATMAIL_ADMIN_WEB_BUILD="$(abspath $(ADMIN_WEB_BUILD))" \
		cargo zigbuild -p chatmail --release --target aarch64-unknown-linux-musl
	@cp target/aarch64-unknown-linux-musl/release/madmail build/madmail-linux-arm64-musl

# First-time assets (iroh-relay, admin-web submodule) then full release publish.
init-publish: init publish

publish: build-publish
	@chmod +x scripts/publish.sh
	@if echo ' $(PUBLISH_ARGS) $(ARGS) ' | grep -q ' init '; then \
		echo 'ℹ️  Ignoring init (Makefile target, not a publish.sh flag). Use: make init publish'; \
	fi
	@./scripts/publish.sh $(_publish_args)

# ── relay-ping (context/relay-ping) ──────────────────────────────────────────
relay-ping-build:
	$(MAKE) -C $(RELAY_PING_DIR) build

relay-ping-clean:
	$(MAKE) -C $(RELAY_PING_DIR) clean

# ── Clean ────────────────────────────────────────────────────────────────────
clean: relay-ping-clean
	cargo clean
	rm -rf build crates/chatmail-admin-web/embed $(ADMIN_WEB_BUILD) context/madmail/internal/adminweb/build
	rm -f $(PID_FILE)
	@echo "Removed Cargo artifacts, admin-web embed staging, and relay-ping bin/"

# ── Manual page (docs/man/madmail.1.scd → docs/man/madmail.1, embedded) ───
MAN_SCD     := docs/man/madmail.1.scd
MAN_ROFF    := docs/man/madmail.1
MAN_VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

man:
	@command -v scdoc >/dev/null || { echo "scdoc required (see docs/man/README.md)" >&2; exit 1; }
	sed 's/@VERSION@/$(MAN_VERSION)/' $(MAN_SCD) | scdoc > $(MAN_ROFF)
	@echo "Wrote $(MAN_ROFF) (version $(MAN_VERSION))"

man-lint: man
	@groff -man -Tutf8 $(MAN_ROFF) >/dev/null
	@echo "groff -man OK: $(MAN_ROFF)"
	@if command -v mandoc >/dev/null 2>&1; then mandoc -T lint $(MAN_ROFF); echo "mandoc -T lint OK"; fi

man-check: man-lint
	@cargo test -p chatmail ctl::docs -- --nocapture

# ── Help ─────────────────────────────────────────────────────────────────────
help:
	@echo "madmail-v2 Makefile (from context/madmail/Makefile)"
	@echo ""
	@echo "Build:     build (Rust only), build-admin-web ($(ADMIN_WEB_DIR) SPA), build-with-admin-web (SPA+embed+Rust), build-release, build-profiling (release + pprof), build-release-static, build-landing ($(LANDING_DIR) static site)"
	@echo "Run:       run, restart, dev-certs, dev-bind-cap (Linux <1024 ports), reset-db, install, preview-landing"
	@echo "Admin UI:  edit $(ADMIN_WEB_DIR) → make build-with-admin-web → make restart"
	@echo "Deploy:    push, push1 (unsigned), push2 (static+sign+upgrade), push-signed, sign (scripts/sign.sh), log1, log2 (scripts/deploy.sh)"
	@echo "Test:      test, test-unit, test-e2e, test-maintenance, test-integration, test-imap, test-turn, test-deltachat, test-dclogin"
	@echo "Quality:   check, lint, fmt, fmt-check, man, man-lint, man-check"
	@echo "relay-ping: relay-ping-build (in $(RELAY_PING_DIR))"
	@echo "Init:      init (download iroh-relay $(IROH_RELAY_VERSION) into $(IROH_ASSETS)/)"
	@echo "Release:   build-publish, publish (PUBLISH_ARGS=…), init-publish (init + publish)"
	@echo "           publish.sh: --no-github-release, --no-release-notes, --sync-keys, …"
	@echo "Other:     clean, help"
	@echo ""
	@echo "Defaults: STATE_DIR=$(STATE_DIR) CONFIG=$(CONFIG)"
	@echo "Push uses: BINARY_PUSH=$(BINARY_PUSH) (remote paths: scripts/deploy.sh)"
