//! Workspace-level integration tests for madmail-v2.

use std::path::PathBuf;
use std::sync::OnceLock;

static CHATMAIL_BIN: OnceLock<PathBuf> = OnceLock::new();

/// Path to the `madmail` binary for the active profile.
///
/// The workspace `tests` crate is separate from `crates/chatmail`, so Cargo does not set
/// `CARGO_BIN_EXE_*`; this helper builds `madmail` on first use if needed.
pub fn chatmail_bin() -> PathBuf {
    CHATMAIL_BIN
        .get_or_init(|| {
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            };
            let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("target")
                .join(profile)
                .join("madmail");
            if !bin.is_file() {
                let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
                let status = std::process::Command::new(&cargo)
                    .args(["build", "-p", "chatmail", "--bin", "madmail"])
                    .status()
                    .expect("spawn cargo to build madmail");
                assert!(status.success(), "failed to build madmail binary");
            }
            bin
        })
        .clone()
}
