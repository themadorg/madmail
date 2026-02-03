# Development

Run common developer tasks from the repository root.

## Make targets

- `make tidy` - run `go mod tidy`
- `make vet` - run `go vet ./...`
- `make lint` - run `golangci-lint run ./...` (requires golangci-lint installed)
- `make test` - run `go test ./...`
- `make build` - build the server and download latest `iroh-relay` binary (v0.35.0)
- `make coverage` - generate `coverage.out` and `coverage.html`
- **[Contributing Guide](../CONTRIBUTING.md)** - Detailed workflow, branching, and PR instructions.
- **[E2E Testing Guide](./chatmail/e2e_test.md)** - Details on running the Python-based test suite.
- **[Iroh Relay Integration](./chatmail/iroh.md)** - Details on the Iroh P2P networking stack.

## Building with Iroh Relay

Madmail integrates the [Iroh Relay](https://iroh.computer/docs/layers/relay) to facilitate WebXDC P2P communication. 

When you run `make build` (or `sh build.sh build`), the build script automatically:
1.  Downloads the correct version of the `iroh-relay` binary (matching the Delta Chat core version) to `internal/endpoint/iroh/assets/`.
2.  Embeds this binary into the `maddy` executable using Go's `embed` package.
3.  Ensures it is extracted and managed as a sidecar during installation.

If you are building in an offline environment, you must manually place the `iroh-relay` binary in `internal/endpoint/iroh/assets/` before building.

## Debugging & Logging

To see detailed logs for debugging, you should enable debug logging in your configuration.

### Enabling Logs
In your `maddy.conf`, ensure the following directives are set:
```
debug yes
log stderr
```
If you are using the `maddy install` command, you can enable this automatically by passing the `--debug` (or `-d`) flag.

### Viewing Logs
- **System-wide install**: Use `journalctl -u maddy -f`
- **Development run**: Logs will be printed directly to `stderr` in your terminal.

## Development Installation (Local)

For development, you might want to install and run maddy without root privileges and without affecting your system-wide installation.

### Local Build & Install
You can use `maddy install` with local paths. When local paths are provided, `maddy` will skip system-level steps like creating a system user or installing systemd service files.

```bash
# Build the binary
go build ./cmd/maddy

# Install to a local directory using absolute paths
./build/maddy install --simple --ip 127.0.0.1 \
    --config-dir $(pwd)/dev/config \
    --state-dir $(pwd)/dev/state \
    --binary-path $(pwd)/dev/maddy \
    --debug
```

### Debug Run Command
After installation, you can run maddy in the foreground for debugging (ensure you use absolute paths for config and libexec):
```bash
sudo ./dev/maddy --config $(pwd)/dev/config/maddy.conf run --libexec $(pwd)/dev/state
```

This setup keeps all maddy files (config, database, etc.) within the specified local directories.

## Notes about SQLite / CGO

- The project supports both `github.com/mattn/go-sqlite3` (requires CGO) and `modernc.org/sqlite` (pure Go) backends. If your system doesn't support CGO, build with the `modernc` driver where relevant or set up appropriate build tags. See `go.mod` for current module pins.
