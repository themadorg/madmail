// Copyright (C) 2026 themadorg
// SPDX-License-Identifier: AGPL-3.0-or-later
//! Source-only server policy for local CLI safeguards across different environments.
use chatmail_config::SettingsPolicy;
use chatmail_types::{ChatmailError, Result};
use std::{collections::BTreeMap, path::Path};
const SNAPSHOT: &str = "configuration-locks.json";

pub fn read(state_dir: &Path) -> Result<BTreeMap<String, String>> {
    match std::fs::read(state_dir.join(SNAPSHOT)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            ChatmailError::config(
                "invalid configuration lock snapshot; restart the service to refresh it",
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.into()),
    }
}

pub fn publish(state_dir: &Path, policy: &SettingsPolicy) -> Result<()> {
    let sources = policy.sources();
    let bytes =
        serde_json::to_vec_pretty(&sources).map_err(|e| ChatmailError::config(e.to_string()))?;
    let temporary = state_dir.join(format!(".{SNAPSHOT}.{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, state_dir.join(SNAPSHOT))?;
    for (key, source) in sources {
        eprintln!("WARNING: {} is protected by {}; database-backed CLI/API changes are disabled. Change that source and restart the service.", chatmail_config::settings_policy::setting_name(&key), source);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_is_source_only_and_replaced_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut policy = SettingsPolicy::default();
        policy
            .insert(
                "turn_secret",
                "never-publish-this",
                "environment:CHATMAIL_TURN_SECRET",
            )
            .unwrap();
        publish(dir.path(), &policy).unwrap();
        assert_eq!(
            read(dir.path()).unwrap()["__TURN_SECRET__"],
            "environment:CHATMAIL_TURN_SECRET"
        );
        assert!(!std::fs::read_to_string(dir.path().join(SNAPSHOT))
            .unwrap()
            .contains("never-publish-this"));
        publish(dir.path(), &SettingsPolicy::default()).unwrap();
        assert!(read(dir.path()).unwrap().is_empty());
    }
    #[test]
    fn corrupt_snapshot_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SNAPSHOT), "invalid json").unwrap();
        assert!(read(dir.path()).is_err());
    }
}
