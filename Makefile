.PHONY: all build install clean test vet lint tidy coverage deploy push publish log1 log2

# Default target
all: build

# Load environment variables
-include .env
export

# Variables
BINARY=build/maddy
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

# Helper to increment version
bump_version:
	@if [ -f $(VERSION_FILE) ]; then \
		awk -F. '{$$NF = $$NF + 1;} 1' OFS=. $(VERSION_FILE) > $(VERSION_FILE).tmp && mv $(VERSION_FILE).tmp $(VERSION_FILE); \
		echo "Version bumped to $$(cat $(VERSION_FILE))"; \
	fi

# Local deployment test
test: build
	sudo systemctl stop maddy.service 
	@echo "Updating local instance (127.0.0.1)"
	sudo ./$(BINARY) install --simple --debug --ip 127.0.0.1
	sudo systemctl start maddy.service

# Remote deployment helper
define deploy_remote
	scp $(BINARY) root@$(1):~/ 
	@echo "Updating remote instance ($(1))" 
	ssh root@$(1) "sudo systemctl stop maddy.service && sudo ./maddy install --simple --debug --ip $(1) && sudo systemctl start maddy.service"
endef

# Push to both servers
push: build
	$(call deploy_remote,$(REMOTE1))
	$(call deploy_remote,$(REMOTE2))

# Publish to Telegram then Arvan (Increment version -> Build -> Script)
# Use ARGS="--publish-no-telegram" to skip Telegram.
publish: bump_version build
	@bash publish.sh $(ARGS)


# Logs
log1: 
	ssh root@$(REMOTE1) "journalctl -u maddy.service -f"

log2: 
	ssh root@$(REMOTE2) "journalctl -u maddy.service -f"

clean:
	rm -rf build coverage.out coverage.html
