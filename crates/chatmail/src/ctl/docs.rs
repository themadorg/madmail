// Copyright (C) 2026 themadorg
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Man page and shell completion generation (Madmail `generate-man` / bash completion parity).

use std::path::{Path, PathBuf};

use chatmail_config::{Args, Cli, CompletionShell};
use chatmail_types::{ChatmailError, Result};
use clap::CommandFactory;
use clap_complete::{generate, shells::Bash};
use clap_complete::{generate as generate_fish, shells::Fish};
use clap_complete::{generate as generate_zsh, shells::Zsh};

use super::output::CtlOut;

/// Rendered from `docs/man/madmail.1.scd` (regenerate with `make man`).
const EMBEDDED_MAN_MADMAIL: &str = include_str!("../../../../docs/man/madmail.1");

/// Basename of argv\[0\] (e.g. `madmail`, `chatmail`).
pub fn argv_binary_name() -> String {
    std::env::args()
        .next()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "madmail".into())
}

pub fn man_roff(binary_name: &str) -> Result<String> {
    Ok(adapt_man_binary_name(EMBEDDED_MAN_MADMAIL, binary_name))
}

/// Rewrite the embedded *madmail*(1) page for another argv[0] basename.
fn adapt_man_binary_name(man: &str, binary_name: &str) -> String {
    if binary_name == "madmail" {
        return man.to_string();
    }
    const REPO_PATH: &str = "themadorg/madmail";
    const REPO_GUARD: &str = "\u{0000}REPO\u{0000}";
    let guarded = man.replace(REPO_PATH, REPO_GUARD);
    let adapted = replace_word(&guarded, "madmail", binary_name);
    adapted.replace(REPO_GUARD, REPO_PATH)
}

fn replace_word(haystack: &str, word: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(idx) = rest.find(word) {
        let (before, _) = rest.split_at(idx);
        out.push_str(before);
        let after = &rest[idx..];
        let after_word = &after[word.len()..];
        let start_ok = before
            .chars()
            .last()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let end_ok = after_word
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if start_ok && end_ok {
            out.push_str(replacement);
            rest = after_word;
        } else {
            out.push_str(word);
            rest = after_word;
        }
    }
    out.push_str(rest);
    out
}

fn named_cli_command(binary_name: &str) -> clap::Command {
    if binary_name == "madmail" {
        return Cli::command();
    }
    let leaked: &'static str = Box::leak(binary_name.to_owned().into_boxed_str());
    Cli::command().name(leaked)
}

pub fn bash_completion(binary_name: &str) -> Result<String> {
    let mut buf = Vec::new();
    let mut cmd = named_cli_command(binary_name);
    generate(Bash, &mut cmd, binary_name, &mut buf);
    String::from_utf8(buf).map_err(|e| ChatmailError::config(format!("bash completion utf8: {e}")))
}

pub fn zsh_completion(binary_name: &str) -> Result<String> {
    let mut buf = Vec::new();
    let mut cmd = named_cli_command(binary_name);
    generate_zsh(Zsh, &mut cmd, binary_name, &mut buf);
    String::from_utf8(buf).map_err(|e| ChatmailError::config(format!("zsh completion utf8: {e}")))
}

pub fn fish_completion(binary_name: &str) -> Result<String> {
    let mut buf = Vec::new();
    let mut cmd = named_cli_command(binary_name);
    generate_fish(Fish, &mut cmd, binary_name, &mut buf);
    String::from_utf8(buf).map_err(|e| ChatmailError::config(format!("fish completion utf8: {e}")))
}

pub fn print_completion(shell: &CompletionShell) -> Result<()> {
    let name = argv_binary_name();
    let script = match shell {
        CompletionShell::Bash => bash_completion(&name)?,
        CompletionShell::Zsh => zsh_completion(&name)?,
        CompletionShell::Fish => fish_completion(&name)?,
    };
    print!("{script}");
    Ok(())
}

pub fn print_generate_man(args: &Args) -> Result<()> {
    let name = argv_binary_name();
    let man = man_roff(&name)?;
    if args.json {
        CtlOut::from_args(args, "generate-man").emit(serde_json::json!({
            "man": man,
            "binary": name,
        }))?;
    } else {
        print!("{man}");
    }
    Ok(())
}

pub fn print_generate_fish_completion(args: &Args) -> Result<()> {
    let name = argv_binary_name();
    let script = fish_completion(&name)?;
    if args.json {
        CtlOut::from_args(args, "generate-fish-completion").emit(serde_json::json!({
            "completion": script,
            "shell": "fish",
            "binary": name,
        }))?;
    } else {
        print!("{script}");
    }
    Ok(())
}

/// Standard FHS paths for man page and shell completions.
pub struct CliDocPaths {
    pub man_page: PathBuf,
    pub bash_completion: PathBuf,
    pub zsh_completion: PathBuf,
    pub fish_completion: PathBuf,
}

impl CliDocPaths {
    pub fn for_binary(binary_name: &str) -> Self {
        Self {
            man_page: PathBuf::from(format!("/usr/share/man/man1/{binary_name}.1")),
            bash_completion: PathBuf::from(format!(
                "/usr/share/bash-completion/completions/{binary_name}"
            )),
            zsh_completion: PathBuf::from(format!("/usr/share/zsh/site-functions/_{binary_name}")),
            fish_completion: PathBuf::from(format!(
                "/usr/share/fish/vendor_completions.d/{binary_name}.fish"
            )),
        }
    }
}

/// Install embedded man page and shell completions (system install only).
pub fn install_cli_docs(binary_name: &str, dry_run: bool) -> Result<()> {
    let paths = CliDocPaths::for_binary(binary_name);
    let man = man_roff(binary_name)?;
    let bash = bash_completion(binary_name)?;
    let zsh = zsh_completion(binary_name)?;
    let fish = fish_completion(binary_name)?;

    if dry_run {
        println!("   Would install man page → {}", paths.man_page.display());
        println!(
            "   Would install bash completion → {}",
            paths.bash_completion.display()
        );
        println!(
            "   Would install zsh completion → {}",
            paths.zsh_completion.display()
        );
        println!(
            "   Would install fish completion → {}",
            paths.fish_completion.display()
        );
        return Ok(());
    }

    write_file(&paths.man_page, man.as_bytes(), 0o644)?;
    write_file(&paths.bash_completion, bash.as_bytes(), 0o755)?;
    write_file(&paths.zsh_completion, zsh.as_bytes(), 0o644)?;
    write_file(&paths.fish_completion, fish.as_bytes(), 0o644)?;

    println!("   ✓ Man page installed ({})", paths.man_page.display());
    println!("   ✓ Shell completions installed (bash, zsh, fish)");

    refresh_man_db();
    Ok(())
}

fn write_file(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ChatmailError::config(format!("mkdir {}: {e}", parent.display())))?;
    }
    std::fs::write(path, contents)
        .map_err(|e| ChatmailError::config(format!("write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| ChatmailError::config(format!("chmod {}: {e}", path.display())))?;
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
    }
    Ok(())
}

fn refresh_man_db() {
    let _ = std::process::Command::new("mandb").arg("-q").status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_man_page_follows_man_pages_sections() {
        for section in [
            ".SH NAME",
            ".SH SYNOPSIS",
            ".SH DESCRIPTION",
            ".SH OPTIONS",
            ".SH EXIT STATUS",
            ".SH ENVIRONMENT",
            ".SH FILES",
            ".SH EXAMPLES",
            ".SH SEE ALSO",
        ] {
            assert!(EMBEDDED_MAN_MADMAIL.contains(section), "missing {section}");
        }
        assert!(EMBEDDED_MAN_MADMAIL.contains(r#".TH "madmail" "1""#));
        assert!(!EMBEDDED_MAN_MADMAIL.contains("SUBCOMMANDS"));
        assert!(!EMBEDDED_MAN_MADMAIL.contains("madmail-run(1)"));
    }

    #[test]
    fn bash_completion_contains_binary_name() {
        let script = bash_completion("madmail").unwrap();
        assert!(script.contains("madmail"));
    }

    #[test]
    fn bash_completion_includes_proxy_subcommand() {
        let script = bash_completion("madmail").unwrap();
        assert!(
            script.contains("proxy"),
            "bash completion should list proxy subcommand"
        );
        assert!(
            script.contains("pr"),
            "bash completion should list proxy alias pr"
        );
    }

    #[test]
    fn bash_completion_prefix_pr_matches_proxy() {
        let script = bash_completion("madmail").unwrap();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("madmail.bash");
        std::fs::write(&path, &script).expect("write completion script");
        let path_s = path.display();
        let result = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                r#"
source "{path_s}"
COMP_WORDS=(madmail pr)
COMP_CWORD=1
_madmail madmail pr ""
printf '%s\n' "${{COMPREPLY[@]}}"
"#
            ))
            .output()
            .expect("run bash completion test");
        assert!(result.status.success(), "bash completion test failed");
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            stdout.lines().any(|line| line == "proxy" || line == "pr"),
            "madmail pr<TAB> should complete to proxy or pr, got: {stdout}"
        );
    }

    #[test]
    fn zsh_completion_includes_proxy_subcommand() {
        let script = zsh_completion("madmail").unwrap();
        assert!(
            script.contains("proxy"),
            "zsh completion should list proxy subcommand"
        );
    }

    #[test]
    fn custom_binary_man_page_adapts_name_and_paths() {
        let man = man_roff("chatmail").unwrap();
        assert!(man.contains(r#".TH "chatmail" "1""#));
        assert!(man.contains(r"\fBchatmail\fR"));
        assert!(man.contains("/var/lib/chatmail"));
        assert!(man.contains("themadorg/madmail"));
        assert!(!man.contains("madmail - Chatmail"));
        assert!(man.contains("chatmail - Chatmail"));
    }

    #[test]
    fn cli_doc_paths_follow_fhs() {
        let paths = CliDocPaths::for_binary("madmail");
        assert_eq!(
            paths.man_page,
            PathBuf::from("/usr/share/man/man1/madmail.1")
        );
        assert_eq!(
            paths.bash_completion,
            PathBuf::from("/usr/share/bash-completion/completions/madmail")
        );
    }
}
