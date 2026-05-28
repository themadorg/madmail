# P1-S04: CLI Argument Parsing

## Action

In `chatmail-config`, add `clap` (`derive`, `env`). Define `Args` with `--config` (default `/etc/chatmail/chatmail.toml`), `--state-dir` (default `/var/lib/chatmail`), `--debug`.

## Files touched

- `crates/chatmail-config/Cargo.toml`
- `crates/chatmail-config/src/cli.rs`
- `crates/chatmail-config/src/lib.rs`

## TDD references

- [14-cli-tools.md](../../TDD/14-cli-tools.md) *(planned)* — CLI surface
- [16-testing.md](../../TDD/16-testing.md) — unit test layout

## Madmail / context references

- `context/madmail/docs/chatmail/commands.md` — global flags

## RFC references

_None._

## Verification

**P1-UT01** `test_cli_defaults_and_overrides` in `cli.rs`.

```bash
cargo test -p chatmail-config test_cli_defaults_and_overrides
```
