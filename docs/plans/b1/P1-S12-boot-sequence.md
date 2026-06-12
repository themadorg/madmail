# P1-S12: Application Boot Sequence

## Action

In `main.rs`: `#[tokio::main]`, parse `Args`, `load_config`, `create_dir_all(state_dir)`, open `{state_dir}/chatmail.db`, apply No-Log, log `madmail-v2 starting`, exit 0.

## Files touched

- `crates/chatmail/src/main.rs`

## TDD references

- [00-intro.md](../../TDD/00-intro.md) — boot/lifecycle
- [01-architecture.md](../../TDD/01-architecture.md) — core process

## Madmail / context references

- `context/madmail/maddy.go` — `Run()`
- `context/madmail/cmd/maddy/main.go`

## RFC references

_None._

## Verification

```bash
cargo run -p chatmail -- --state-dir ./data
```
