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

use base64::{engine::general_purpose::STANDARD, Engine};
use chatmail_types::{ChatmailError, Result};
use sha_crypt::{PasswordVerifier, ShaCrypt};
use sha2::{Digest, Sha256};

/// Hash a password for storage (`bcrypt:...` or `argon2:...`).
pub fn hash_password(password: &str) -> Result<String> {
    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|e| ChatmailError::config(e.to_string()))?;
    Ok(format!("bcrypt:{hash}"))
}

/// Verify password against stored hash (bcrypt, argon2, madmail sha256, or POSIX sha-crypt $5$/$6$).
pub fn verify_password(password: &str, stored: &str) -> Result<bool> {
    if let Some(hash) = stored.strip_prefix("bcrypt:") {
        return Ok(bcrypt::verify(password, hash).unwrap_or(false));
    }
    if let Some(rest) = stored.strip_prefix("argon2:") {
        return Ok(verify_argon2(password, rest).unwrap_or(false));
    }
    if let Some(rest) = stored.strip_prefix("sha256:") {
        return Ok(verify_sha256(password, rest).unwrap_or(false));
    }
    // POSIX SHA-256-crypt ($5$) and SHA-512-crypt ($6$)
    if stored.starts_with("$6$") || stored.starts_with("$5$") {
        return Ok(verify_sha_crypt(password, stored).unwrap_or(false));
    }
    // Legacy: raw bcrypt hash
    if stored.starts_with("$2") {
        return Ok(bcrypt::verify(password, stored).unwrap_or(false));
    }
    Ok(false)
}

/// Whether `stored` uses a hash format accepted by import APIs (admin/CLI account import).
pub fn is_importable_hash(stored: &str) -> bool {
    !stored.is_empty()
        && (stored.starts_with("bcrypt:")
            || stored.starts_with("argon2:")
            || stored.starts_with("sha256:")
            || stored.starts_with("$6$")
            || stored.starts_with("$5$")
            || stored.starts_with("$2"))
}

/// Madmail `pass_table` SHA256: `sha256:<salt_b64>:<hash_b64>` where hash = SHA256(salt || password).
fn verify_sha256(password: &str, hash_salt: &str) -> std::result::Result<bool, ()> {
    let (salt_b64, hash_b64) = hash_salt.split_once(':').ok_or(())?;
    let salt = STANDARD.decode(salt_b64).map_err(|_| ())?;
    let expected = STANDARD.decode(hash_b64).map_err(|_| ())?;
    let mut input = salt;
    input.extend_from_slice(password.as_bytes());
    let sum = Sha256::digest(&input);
    Ok(sum.as_slice() == expected.as_slice())
}

fn verify_argon2(password: &str, spec: &str) -> std::result::Result<bool, ()> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 5 {
        return Err(());
    }
    let _time: u32 = parts[0].parse().map_err(|_| ())?;
    let memory: u32 = parts[1].parse().map_err(|_| ())?;
    let threads: u32 = parts[2].parse().map_err(|_| ())?;
    let salt = STANDARD.decode(parts[3]).map_err(|_| ())?;
    let expected = STANDARD.decode(parts[4]).map_err(|_| ())?;
    let params =
        argon2::Params::new(memory, threads, 1, Some(expected.len())).map_err(|_| ())?;
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut output = vec![0u8; expected.len()];
    argon2
        .hash_password_into(password.as_bytes(), &salt, &mut output)
        .map_err(|_| ())?;
    Ok(output == expected)
}

fn verify_sha_crypt(password: &str, stored: &str) -> std::result::Result<bool, ()> {
    let hasher = if stored.starts_with("$6$") {
        ShaCrypt::default()
    } else {
        ShaCrypt::SHA256
    };
    Ok(hasher
        .verify_password(password.as_bytes(), stored)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P3-UT02
    #[test]
    fn p3_ut02_test_bcrypt_verify() {
        let stored = hash_password("secret-pass").unwrap();
        assert!(verify_password("secret-pass", &stored).unwrap());
        assert!(!verify_password("wrong", &stored).unwrap());
    }

    #[test]
    fn madmail_sha256_imported_hash() {
        let stored = "sha256:j/cpuNy8vAXCe5U/hSJCp9vcGNPkWcozSkBPEsDwOTg=:UZZle/gjww6zKMVtY4SUf+Zh2c1lwsDBWpxSJp+nUI0=";
        assert!(!verify_password("wrong", stored).unwrap());
    }

    #[test]
    fn sha512_crypt_login() {
        let stored = "$6$testsalt$zcc0po6c786cz9LdMIli0E4Zox6uXK6Khb536rxCF/JO..UDVYHeg9zCKnpkm0FyMFumVno4DCKiS8pQLicRP.";
        assert!(verify_password("testpass", stored).unwrap());
        assert!(!verify_password("wrong", stored).unwrap());
        assert!(is_importable_hash(stored));
    }

    #[test]
    fn sha256_crypt_login() {
        let stored = "$5$testsalt$GR6PqdknD2fHavVjM//Q.4Qni8EXZKnxS838p5GC9r5";
        assert!(verify_password("testpass", stored).unwrap());
        assert!(!verify_password("wrong", stored).unwrap());
        assert!(is_importable_hash(stored));
    }
}
