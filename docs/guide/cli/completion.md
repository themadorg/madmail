# `madmail completion`

Print shell tab-completion scripts for bash, zsh, and fish.

## Usage

```bash
madmail completion bash
madmail completion zsh
madmail completion fish
```

Redirect stdout to the appropriate system path (requires root), or rely on `madmail install` to install completions automatically on system installs.

## System install paths

| Shell | Install path |
|-------|----------------|
| bash | `/usr/share/bash-completion/completions/<binary>` |
| zsh | `/usr/share/zsh/site-functions/_<binary>` |
| fish | `/usr/share/fish/vendor_completions.d/<binary>.fish` |

`<binary>` is the executable basename (`madmail`, `chatmail`, …).

## Madmail-compatible hidden helpers

| Command | Purpose |
|---------|---------|
| `generate-man` | Print roff man page (embedded at build time) |
| `generate-fish-completion` | Print fish completion (alias for `completion fish`) |

## JSON output

With `--json`, hidden helpers emit the generated text in the `data` envelope field.

## See also

- [install](../install.md) — installs man page and completions on system install
- [global-flags.md](global-flags.md)

[Source: `crates/chatmail/src/ctl/docs.rs`](https://github.com/themadorg/madmail/blob/main/crates/chatmail/src/ctl/docs.rs)
