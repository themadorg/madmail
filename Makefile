.PHONY: all build install clean test vet lint tidy coverage deploy push publish sign log1 log2 dev-web

# Default target
all: build

# Load environment variables
-include .env
export

# Variables
BINARY=build/maddy
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
	scp $(BINARY) root@$(1):~/maddy-new
	@echo "Upgrading remote instance ($(1))"
	ssh root@$(1) "sudo /usr/local/bin/maddy upgrade ~/maddy-new && rm ~/maddy-new"
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

clean:
	rm -rf build coverage.out coverage.html
