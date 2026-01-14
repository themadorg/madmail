# Development

Run common developer tasks from the repository root.

Make targets:

- `make tidy` - run `go mod tidy`
- `make vet` - run `go vet ./...`
- `make lint` - run `golangci-lint run ./...` (requires golangci-lint installed)
- `make test` - run `go test ./...`
- `make coverage` - generate `coverage.out` and `coverage.html`

Notes about SQLite / CGO:

- The project supports both `github.com/mattn/go-sqlite3` (requires CGO) and `modernc.org/sqlite` (pure Go) backends. If your system doesn't support CGO, build with the `modernc` driver where relevant or set up appropriate build tags. See `go.mod` for current module pins.
