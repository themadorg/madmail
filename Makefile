.PHONY: all build install clean test vet lint tidy coverage deploy push publish sign log1 log2 dev-web profile profile-build profile-push profile-do profile-cmping profile-cmping-multi

# Default target
all: build

# Load environment variables
-include .env
export

# Variables
BINARY=build/maddy
BINARY_PROFILE=build/maddy-profile
BINARY_AMD64=build/maddy-amd64
BINARY_AMD64_LEGACY=build/maddy-amd64-legacy
BINARY_ARM64=build/maddy-arm64
VERSION_FILE=.version
# Unit tests
test-unit:
	go test ./...

# Vet and lint
vet:
	go vet ./...

tidy:
	go mod tidy

lint:
	golangci-lint run ./...

coverage:
	go test ./... -coverprofile=coverage.out
	go tool cover -html=coverage.out -o coverage.html

# Build target
build:
	sh build.sh build

build_all:
	@echo "🏗️ Building for x86_64..."
	GOARCH=amd64 sh build.sh build
	@echo "🏗️ Building for Raspberry Pi (ARM64)..."
	GOARCH=arm64 CGO_ENABLED=0 sh build.sh build

build_legacy:
	@echo "🏗️ Building legacy static binary for x86_64 (CGO_ENABLED=0)..."
	CGO_ENABLED=0 GOARCH=amd64 go build -tags "osusergo netgo static_build" \
		-trimpath -ldflags="-s -w -X \"github.com/themadorg/madmail/framework/config.Version=$$(cat .version)+$$(git rev-parse --short HEAD)\"" \
		-o $(BINARY_AMD64_LEGACY) ./cmd/maddy
	@echo "✅ Legacy binary: $(BINARY_AMD64_LEGACY) (statically linked, no GLIBC dependency)"

build_all_with_legacy: build_all build_legacy
	@echo "✅ All binaries built (amd64, amd64-legacy, arm64)"

test:
	uv run python3 tests/deltachat-test/main.py --lxc

# Helper to increment version
bump_version:
	@if [ -f $(VERSION_FILE) ]; then \
		awk -F. '{$$NF = $$NF + 1;} 1' OFS=. $(VERSION_FILE) > $(VERSION_FILE).tmp && mv $(VERSION_FILE).tmp $(VERSION_FILE); \
		echo "Version bumped to $$(cat $(VERSION_FILE))"; \
	fi

install: build
	sudo systemctl stop maddy.service 
	@echo "Updating local instance (127.0.0.1)"
	sudo ./$(BINARY) install --simple --ip 127.0.0.1
	sudo systemctl start maddy.service

# Signing key for dev deployments (same path as publish.sh)
PRIV_KEY_FILE ?= ../imp/private_key.hex

# Sign the binary for deployment
sign:
	@echo "🔏 Signing binary..."
	@uv run internal/cli/clitools/sign.py $(BINARY) $(PRIV_KEY_FILE)

# Remote deployment helper (sign → upload → upgrade with signature verification)
define deploy_remote
	scp $(BINARY) root@$(1):~/madmail-new
	@echo "Upgrading remote instance ($(1))"
	ssh root@$(1) "sudo /usr/local/bin/madmail upgrade ~/madmail-new && rm ~/madmail-new"
endef

# Push to both servers (sign once, deploy to both)
push: build sign
	$(call deploy_remote,$(REMOTE1))
	$(call deploy_remote,$(REMOTE2))

# Publish to Telegram then GitHub (Increment version -> Build -> Sign -> Script)
# Use ARGS="--publish-no-telegram" to skip Telegram.
publish: build_all
	@bash publish.sh $(ARGS)

sign_all: build_all
	@echo "🔏 Signing binaries..."
	@if [ -f $(BINARY_AMD64) ] && [ -n "$(PRIV_KEY)" ]; then uv run internal/cli/clitools/sign.py $(BINARY_AMD64) $(PRIV_KEY); fi
	@if [ -f $(BINARY_AMD64_LEGACY) ] && [ -n "$(PRIV_KEY)" ]; then uv run internal/cli/clitools/sign.py $(BINARY_AMD64_LEGACY) $(PRIV_KEY); fi
	@if [ -f $(BINARY_ARM64) ] && [ -n "$(PRIV_KEY)" ]; then uv run internal/cli/clitools/sign.py $(BINARY_ARM64) $(PRIV_KEY); fi
	@if [ -f $(BINARY) ] && [ -n "$(PRIV_KEY)" ]; then uv run internal/cli/clitools/sign.py $(BINARY) $(PRIV_KEY); fi


# Logs
log1: 
	ssh root@$(REMOTE1) "journalctl -u maddy.service -f"

log2: 
	ssh root@$(REMOTE2) "journalctl -u maddy.service -f"


# Dev server for previewing chatmail web templates (edit & reload)
DEV_WEB_PORT ?= 3000
dev-web:
	@cd internal/endpoint/chatmail/www && go run devserver.go -port $(DEV_WEB_PORT)

# ── Profiling ─────────────────────────────────────────────────────────
# Build with debug symbols + pprof endpoint (no stripping, full DWARF info)
profile-build:
	@echo "🔬 Building profiling binary..."
	go build -gcflags="all=-N -l" \
		-ldflags="-X \"github.com/themadorg/madmail/framework/config.Version=$$(cat .version)+$$(git rev-parse --short HEAD)-profile\"" \
		-o $(BINARY_PROFILE) ./cmd/maddy-profile
	@echo "✅ Profiling binary: $(BINARY_PROFILE)"

# Deploy the profiling binary to servers (sign → upload → upgrade)
profile-push: profile-build sign-profile
	$(call deploy_remote_profile,$(REMOTE1))

sign-profile:
	@echo "🔏 Signing profiling binary..."
	@uv run internal/cli/clitools/sign.py $(BINARY_PROFILE) $(PRIV_KEY_FILE)

define deploy_remote_profile
	scp $(BINARY_PROFILE) root@$(1):~/maddy-new
	@echo "🔬 Upgrading remote instance ($(1)) with profiling binary"
	ssh root@$(1) "sudo /usr/local/bin/maddy upgrade ~/maddy-new && rm ~/maddy-new"
endef

# ── Profile: all-in-one (build → push → pprof + cmping in parallel) ───
PROFILE_WINDOW ?= 60
profile: profile-push
	@echo "🚀 Starting profiling session ($(PROFILE_WINDOW)s window)..."
	@echo "   pprof capture + cmping load running in parallel"
	@go tool pprof -http=:8081 http://$(PROFILE_TARGET):$(PPROF_PORT)/debug/pprof/profile?seconds=$(PROFILE_WINDOW) &
	@sleep 2
	@$(MAKE) profile-cmping CMPING_COUNT=$$(echo '$(PROFILE_WINDOW) / $(CMPING_INTERVAL)' | bc | cut -d. -f1)
	@echo "✅ Profiling session complete. pprof UI at http://localhost:8081"
	@echo "   Press Ctrl+C to stop the pprof web server."
	@wait

# Run pprof against remote server
PPROF_PORT ?= 6666
PPROF_SECONDS ?= 60
PROFILE_TARGET ?= $(REMOTE1)
profile-do:
	@echo "🔬 Profiling $(PROFILE_TARGET):$(PPROF_PORT) for $(PPROF_SECONDS)s..."
	go tool pprof -http=:8081 http://$(PROFILE_TARGET):$(PPROF_PORT)/debug/pprof/profile?seconds=$(PPROF_SECONDS)

# Hammer the server: 1 sender → 10 receivers, N messages, fast interval
CMPING_RECIPIENTS ?= 10
CMPING_COUNT ?= 200
CMPING_INTERVAL ?= 0.3
profile-cmping:
	@echo "🔨 cmping: $(CMPING_COUNT) msgs → $(CMPING_RECIPIENTS) receivers @ $(CMPING_INTERVAL)s interval"
	uv run python3 cmping/cmping.py $(PROFILE_TARGET) -g $(CMPING_RECIPIENTS) -c $(CMPING_COUNT) -i $(CMPING_INTERVAL)

# Parallel cmping: N isolated subprocesses (each with separate XDG_CACHE_HOME)
CMPING_PROCS ?= 10
profile-cmping-multi:
	@echo "🔨 Launching $(CMPING_PROCS) parallel cmping processes ($(CMPING_COUNT) msgs, $(CMPING_RECIPIENTS) receivers each)..."
	@rm -rf /tmp/cmping-profile
	@for i in $$(seq 1 $(CMPING_PROCS)); do \
		echo "  → Starting cmping subprocess $$i/$(CMPING_PROCS) (cache: /tmp/cmping-profile/$$i)"; \
		XDG_CACHE_HOME=/tmp/cmping-profile/$$i uv run python3 cmping/cmping.py $(PROFILE_TARGET) -g $(CMPING_RECIPIENTS) -c $(CMPING_COUNT) -i $(CMPING_INTERVAL) & \
	done; \
	echo "🔬 All $(CMPING_PROCS) subprocesses launched. Waiting..."; \
	wait; \
	echo "✅ All cmping subprocesses finished."

clean:
	rm -rf build coverage.out coverage.html
