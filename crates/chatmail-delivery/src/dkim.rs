// Copyright (C) 2026 themadorg
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Outbound DKIM (RFC 6376) for federation `/mxdeliv` and SMTP fallback.
//!
//! cmdeploy `filtermail` rejects unsigned mail with
//! `554 5.7.1 No DKIM signature found` (HTTP 400 on `/mxdeliv`). Signatures
//! are produced with **viadkim** — the same crate filtermail uses to verify —
//! so a published `default._domainkey` TXT is enough for alignment + crypto.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use tracing::{debug, warn};
use viadkim::signature::{Canonicalization, CanonicalizationAlgorithm};
use viadkim::signer::{Expiration, SignRequest};
use viadkim::{DomainName, HeaderFields, Selector, SigningAlgorithm, SigningKey};

use chatmail_types::is_ipv4_literal;

/// Install / runtime selector (matches `dkim … default` in generated madmail.conf).
pub const DKIM_SELECTOR: &str = "default";

/// Why IP-literal mail domains are left unsigned (filtermail treats that as a no-op).
pub const IP_SIGNING_REASON: &str =
    "DKIM d= cannot be an IP literal; use a DNS mail domain, then publish default._domainkey";

/// RSA-SHA256 signer for one selector + domain (`d=`).
#[derive(Clone)]
pub struct DkimSigner {
    key: Arc<SigningKey>,
    pub domain: String,
    pub selector: String,
}

impl DkimSigner {
    /// Load `{state}/dkim/{selector}.private` or generate a 2048-bit key + TXT file.
    pub fn load_or_create(
        state_dir: &Path,
        selector: &str,
        primary_domain: &str,
    ) -> Result<Self, String> {
        let domain = signing_domain(primary_domain).ok_or_else(|| {
            format!("DKIM d= cannot be an IP literal ({primary_domain}); publish a DNS name")
        })?;
        let dir = dkim_dir(state_dir);
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        let key_path = private_key_path(state_dir, selector);
        let rsa_key = if key_path.is_file() {
            let pem = fs::read_to_string(&key_path)
                .map_err(|e| format!("read {}: {e}", key_path.display()))?;
            RsaPrivateKey::from_pkcs8_pem(&pem)
                .map_err(|e| format!("parse {}: {e}", key_path.display()))?
        } else {
            let mut rng = rand::thread_rng();
            let generated = RsaPrivateKey::new(&mut rng, 2048)
                .map_err(|e| format!("generate DKIM key: {e}"))?;
            write_private_key(&key_path, &generated)?;
            write_public_txt(state_dir, selector, &generated)?;
            generated
        };
        // Refresh TXT so operators can copy it after upgrades.
        let _ = write_public_txt(state_dir, selector, &rsa_key);
        Ok(Self {
            key: Arc::new(SigningKey::Rsa(rsa_key)),
            domain,
            selector: selector.to_string(),
        })
    }

    /// Prepend `DKIM-Signature` unless the message is already signed, `From` is
    /// an IP literal, or `From`/`MAIL FROM` is not this signer's domain
    /// (filtermail requires `d=` == From domain).
    pub async fn sign_message(&self, raw: &[u8], mail_from: &str) -> Vec<u8> {
        if has_header(raw, "dkim-signature") {
            return raw.to_vec();
        }
        let crlf = to_crlf(raw);
        let Some(d) = aligned_signing_domain(&crlf, mail_from, &self.domain) else {
            debug!("skip DKIM: no DNS From/MAIL FROM aligned with signing domain");
            return raw.to_vec();
        };
        match sign_with_viadkim(&self.key, &d, &self.selector, &crlf).await {
            Ok(signed) => signed,
            Err(e) => {
                warn!(error = %e, "DKIM sign failed; sending unsigned");
                raw.to_vec()
            }
        }
    }

    pub fn public_txt(&self, state_dir: &Path) -> Result<String, String> {
        fs::read_to_string(public_txt_path(state_dir, &self.selector))
            .map(|s| s.trim().to_string())
            .map_err(|e| e.to_string())
    }
}

pub fn dkim_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("dkim")
}

pub fn private_key_path(state_dir: &Path, selector: &str) -> PathBuf {
    dkim_dir(state_dir).join(format!("{selector}.private"))
}

pub fn public_txt_path(state_dir: &Path, selector: &str) -> PathBuf {
    dkim_dir(state_dir).join(format!("{selector}.txt"))
}

/// Selector, `d=`, paths, and TXT — same payload as `madmail dkim show` / `GET /admin/dkim`.
///
/// Creates `{state}/dkim/{selector}.private` when missing and `primary_domain` is a DNS name.
pub fn publish_info(state_dir: &Path, primary_domain: &str) -> Result<Value, String> {
    let selector = DKIM_SELECTOR;
    let private_path = private_key_path(state_dir, selector);
    let txt_file = public_txt_path(state_dir, selector);
    let dns_name = format!("{selector}._domainkey");
    let Some(domain) = signing_domain(primary_domain) else {
        return Ok(json!({
            "selector": selector,
            "domain": primary_domain,
            "dns_name": dns_name,
            "dns_fqdn": Value::Null,
            "private_key_path": private_path.display().to_string(),
            "txt_path": txt_file.display().to_string(),
            "txt": Value::Null,
            "key_present": private_path.is_file(),
            "generated": false,
            "publishable": false,
            "reason": IP_SIGNING_REASON,
        }));
    };
    let existed = private_path.is_file();
    let signer = DkimSigner::load_or_create(state_dir, selector, &domain)?;
    let txt = signer.public_txt(state_dir)?;
    let dns_fqdn = format!("{dns_name}.{}", signer.domain);
    Ok(json!({
        "selector": selector,
        "domain": signer.domain,
        "dns_name": dns_name,
        "dns_fqdn": dns_fqdn,
        "private_key_path": private_path.display().to_string(),
        "txt_path": txt_file.display().to_string(),
        "txt": txt,
        "key_present": true,
        "generated": !existed,
        "publishable": true,
    }))
}

/// Like [`publish_info`] but never writes a key (for `madmail dkim status`).
pub fn inspect_info(state_dir: &Path, primary_domain: &str) -> Result<Value, String> {
    let selector = DKIM_SELECTOR;
    let private_path = private_key_path(state_dir, selector);
    let txt_file = public_txt_path(state_dir, selector);
    let dns_name = format!("{selector}._domainkey");
    let Some(domain) = signing_domain(primary_domain) else {
        return Ok(json!({
            "selector": selector,
            "domain": primary_domain,
            "dns_name": dns_name,
            "dns_fqdn": Value::Null,
            "private_key_path": private_path.display().to_string(),
            "txt_path": txt_file.display().to_string(),
            "txt": Value::Null,
            "key_present": private_path.is_file(),
            "generated": false,
            "publishable": false,
            "reason": IP_SIGNING_REASON,
        }));
    };
    let dns_fqdn = format!("{dns_name}.{domain}");
    if !private_path.is_file() {
        return Ok(json!({
            "selector": selector,
            "domain": domain,
            "dns_name": dns_name,
            "dns_fqdn": dns_fqdn,
            "private_key_path": private_path.display().to_string(),
            "txt_path": txt_file.display().to_string(),
            "txt": Value::Null,
            "key_present": false,
            "generated": false,
            "publishable": false,
            "reason": "no DKIM key yet; run madmail dkim show (or install / first outbound send)",
        }));
    }
    let txt = fs::read_to_string(&txt_file)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    Ok(json!({
        "selector": selector,
        "domain": domain,
        "dns_name": dns_name,
        "dns_fqdn": dns_fqdn,
        "private_key_path": private_path.display().to_string(),
        "txt_path": txt_file.display().to_string(),
        "txt": if txt.is_empty() { Value::Null } else { json!(txt) },
        "key_present": true,
        "generated": false,
        "publishable": !txt.is_empty(),
    }))
}

/// Local key + DNS match for `madmail dkim status` (does not create a key).
pub async fn status_info(state_dir: &Path, primary_domain: &str) -> Result<Value, String> {
    let mut data = inspect_info(state_dir, primary_domain)?;
    let publishable = data["publishable"].as_bool().unwrap_or(false);
    let txt = data["txt"].as_str().unwrap_or("").to_string();
    let fqdn = data["dns_fqdn"].as_str().unwrap_or("").to_string();
    if !publishable || txt.is_empty() || fqdn.is_empty() {
        data["dns_checked"] = json!(false);
        data["dns_matched"] = json!(false);
        data["dns_txt"] = json!([]);
        return Ok(data);
    }
    match lookup_txt(fqdn).await {
        Ok(recs) => {
            data["dns_checked"] = json!(true);
            data["dns_matched"] = json!(dkim_txt_matches(&txt, &recs));
            data["dns_txt"] = json!(recs);
        }
        Err(e) => {
            data["dns_checked"] = json!(true);
            data["dns_matched"] = json!(false);
            data["dns_txt"] = json!([]);
            data["lookup_error"] = json!(e);
        }
    }
    Ok(data)
}

/// Collapse DNS quoting/whitespace; keep `p=` base64 case.
pub fn normalize_dkim_txt(s: &str) -> String {
    let compact: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '"')
        .collect();
    compact
        .split(';')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let Some((k, v)) = part.split_once('=') else {
                return Some(part.to_ascii_lowercase());
            };
            if k.eq_ignore_ascii_case("p") {
                Some(format!("p={v}"))
            } else {
                Some(format!(
                    "{}={}",
                    k.to_ascii_lowercase(),
                    v.to_ascii_lowercase()
                ))
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub fn dkim_txt_matches(expected: &str, found: &[String]) -> bool {
    let want = normalize_dkim_txt(expected);
    !want.is_empty() && found.iter().any(|f| normalize_dkim_txt(f) == want)
}

/// TXT lookup for `name` (FQDN). Empty vec = NXDOMAIN / no TXT.
pub async fn lookup_txt(name: String) -> Result<Vec<String>, String> {
    use hickory_resolver::config::{ResolverConfig, ResolverOpts};
    use hickory_resolver::TokioAsyncResolver;

    let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
        Ok(r) => r,
        Err(_) => TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), ResolverOpts::default()),
    };
    let qname = name.trim_end_matches('.').to_string();
    match resolver.txt_lookup(qname).await {
        Ok(lookup) => Ok(lookup
            .iter()
            .map(|txt| {
                txt.txt_data()
                    .iter()
                    .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect()),
        Err(e) => {
            let msg = e.to_string();
            let empty = matches!(
                e.kind(),
                hickory_resolver::error::ResolveErrorKind::NoRecordsFound { .. }
            ) || msg.to_ascii_lowercase().contains("nxdomain")
                || msg.to_ascii_lowercase().contains("no records");
            if empty {
                Ok(Vec::new())
            } else {
                Err(msg)
            }
        }
    }
}

/// Compare local DKIM TXT to the published `default._domainkey` record.
pub async fn check_dns(state_dir: &Path, primary_domain: &str) -> Result<Value, String> {
    check_dns_with(state_dir, primary_domain, lookup_txt).await
}

pub async fn check_dns_with<F, Fut>(
    state_dir: &Path,
    primary_domain: &str,
    lookup: F,
) -> Result<Value, String>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<String>, String>>,
{
    let info = publish_info(state_dir, primary_domain)?;
    let publishable = info["publishable"].as_bool().unwrap_or(false);
    let selector = info["selector"].clone();
    let domain = info["domain"].clone();
    let dns_name = info["dns_name"].clone();
    let dns_fqdn = info["dns_fqdn"].clone();
    let expected = info["txt"].clone();
    if !publishable {
        return Ok(json!({
            "selector": selector,
            "domain": domain,
            "dns_name": dns_name,
            "dns_fqdn": dns_fqdn,
            "expected_txt": expected,
            "dns_txt": Value::Array(vec![]),
            "matched": false,
            "checked": false,
            "reason": info.get("reason").cloned().unwrap_or(Value::Null),
        }));
    }
    let fqdn = dns_fqdn
        .as_str()
        .ok_or_else(|| "missing dns_fqdn".to_string())?
        .to_string();
    match lookup(fqdn.clone()).await {
        Ok(recs) => {
            let expected_s = expected.as_str().unwrap_or("");
            let matched = dkim_txt_matches(expected_s, &recs);
            Ok(json!({
                "selector": selector,
                "domain": domain,
                "dns_name": dns_name,
                "dns_fqdn": fqdn,
                "expected_txt": expected,
                "dns_txt": recs,
                "matched": matched,
                "checked": true,
            }))
        }
        Err(e) => Ok(json!({
            "selector": selector,
            "domain": domain,
            "dns_name": dns_name,
            "dns_fqdn": fqdn,
            "expected_txt": expected,
            "dns_txt": Value::Array(vec![]),
            "matched": false,
            "checked": true,
            "lookup_error": e,
        })),
    }
}

/// DNS name for `d=`, or `None` for empty / IPv4 / IPv6 / bracketed IP.
pub fn signing_domain(addr_or_domain: &str) -> Option<String> {
    let s = addr_or_domain.trim();
    let domain = s
        .rsplit_once('@')
        .map(|(_, d)| d)
        .unwrap_or(s)
        .trim()
        .trim_matches(|c| c == '[' || c == ']');
    if domain.is_empty() || is_ipv4_literal(domain) || domain.contains(':') {
        return None;
    }
    if !domain.contains('.') {
        return None;
    }
    Some(domain.to_ascii_lowercase())
}

fn aligned_signing_domain(raw: &[u8], mail_from: &str, signer_domain: &str) -> Option<String> {
    let candidate = from_header_domain(raw).or_else(|| signing_domain(mail_from))?;
    if candidate.eq_ignore_ascii_case(signer_domain) {
        Some(signer_domain.to_ascii_lowercase())
    } else {
        None
    }
}

fn extract_addr(from_value: &str) -> Option<&str> {
    let v = from_value.trim();
    if let Some(start) = v.rfind('<') {
        let end = v.rfind('>')?;
        if end > start {
            return Some(v[start + 1..end].trim());
        }
    }
    Some(v)
}

fn write_private_key(path: &Path, key: &RsaPrivateKey) -> Result<(), String> {
    let pem = key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(|e| format!("serialize DKIM key: {e}"))?;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    f.write_all(pem.as_bytes())
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

fn write_public_txt(state_dir: &Path, selector: &str, key: &RsaPrivateKey) -> Result<(), String> {
    let path = public_txt_path(state_dir, selector);
    let txt = public_txt_record(key)?;
    fs::write(&path, format!("{txt}\n")).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

fn public_txt_record(key: &RsaPrivateKey) -> Result<String, String> {
    let der = key
        .to_public_key()
        .to_pkcs1_der()
        .map_err(|e| format!("DKIM public key: {e}"))?;
    let p = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, der.as_bytes());
    Ok(format!("v=DKIM1; k=rsa; p={p}"))
}

async fn sign_with_viadkim(
    key: &SigningKey,
    domain: &str,
    selector: &str,
    crlf: &[u8],
) -> Result<Vec<u8>, String> {
    let (header_block, body) = split_message(crlf)?;
    let header_str = std::str::from_utf8(header_block)
        .map_err(|_| "message headers are not UTF-8".to_string())?;
    let headers: HeaderFields = header_str
        .parse()
        .map_err(|e| format!("parse headers for DKIM: {e}"))?;
    let domain = DomainName::new(domain).map_err(|e| format!("DKIM d=: {e}"))?;
    let selector = Selector::new(selector).map_err(|e| format!("DKIM s=: {e}"))?;
    let mut request = SignRequest::new(domain, selector, SigningAlgorithm::RsaSha256, key);
    request.canonicalization = Canonicalization::from((
        CanonicalizationAlgorithm::Relaxed,
        CanonicalizationAlgorithm::Relaxed,
    ));
    // No x= — federation retries can sit in queue; clock skew must not expire the sig.
    request.expiration = Expiration::Never;
    let results = viadkim::sign(headers, body, [request])
        .await
        .map_err(|e| format!("DKIM sign request: {e}"))?;
    let signature = results
        .into_iter()
        .next()
        .ok_or_else(|| "DKIM sign produced no result".to_string())?
        .map_err(|e| format!("DKIM sign: {e}"))?;
    let mut hdr = signature.format_header().to_string();
    if !hdr.ends_with("\r\n") {
        hdr.push_str("\r\n");
    }
    let mut out = hdr.into_bytes();
    out.extend_from_slice(crlf);
    Ok(out)
}

fn has_header(raw: &[u8], name: &str) -> bool {
    split_message(raw).ok().is_some_and(|(h, _)| {
        parse_headers(h)
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    })
}

fn split_message(raw: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| (i, 4))
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n").map(|i| (i, 2)))
        .ok_or_else(|| "message has no header/body separator".to_string())?;
    Ok((&raw[..sep.0], &raw[sep.0 + sep.1..]))
}

fn parse_headers(block: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(block);
    let mut out = Vec::new();
    let mut cur_name = String::new();
    let mut cur_val = String::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            if !cur_name.is_empty() {
                cur_val.push(' ');
                cur_val.push_str(line.trim());
            }
            continue;
        }
        if !cur_name.is_empty() {
            out.push((std::mem::take(&mut cur_name), std::mem::take(&mut cur_val)));
        }
        if let Some((n, v)) = line.split_once(':') {
            cur_name = n.trim().to_string();
            cur_val = v.trim().to_string();
        }
    }
    if !cur_name.is_empty() {
        out.push((cur_name, cur_val));
    }
    out
}

fn to_crlf(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + 16);
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\r' && i + 1 < raw.len() && raw[i + 1] == b'\n' {
            out.extend_from_slice(b"\r\n");
            i += 2;
        } else if raw[i] == b'\n' {
            out.extend_from_slice(b"\r\n");
            i += 1;
        } else {
            out.push(raw[i]);
            i += 1;
        }
    }
    out
}

fn from_header_domain(raw: &[u8]) -> Option<String> {
    let (header_block, _) = split_message(raw).ok()?;
    let headers = parse_headers(header_block);
    let (_, val) = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("from"))?;
    let addr = extract_addr(val)?;
    signing_domain(addr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use viadkim::message_hash::BodyHasherStance;
    use viadkim::verifier::LookupTxt;
    use viadkim::{Config, VerificationStatus, Verifier};

    fn sample_msg(from: &str) -> Vec<u8> {
        format!(
            "From: <{from}>\r\nTo: <bob@cm.example>\r\nSubject: hi\r\n\
             Date: Sun, 16 Aug 2026 12:00:00 +0000\r\nMessage-ID: <a@b>\r\n\
             Content-Type: multipart/encrypted; boundary=\"b\"\r\n\r\n\
             --b\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n\
             --b\r\nContent-Type: application/octet-stream\r\n\r\nx\r\n--b--\r\n"
        )
        .into_bytes()
    }

    #[derive(Clone)]
    struct MockTxt(String);

    impl LookupTxt for MockTxt {
        type Answer = std::vec::IntoIter<io::Result<Vec<u8>>>;
        type Query<'a> = Pin<Box<dyn Future<Output = io::Result<Self::Answer>> + Send + 'a>>;

        fn lookup_txt(&self, _domain: &str) -> Self::Query<'_> {
            let txt = self.0.clone();
            Box::pin(async move { Ok(vec![Ok(txt.into_bytes())].into_iter()) })
        }
    }

    async fn viadkim_accepts(signed: &[u8], txt: &str, from_domain: &str) {
        let text = std::str::from_utf8(signed).expect("utf8");
        let (header, body) = text.split_once("\r\n\r\n").expect("separator");
        let header: HeaderFields = header.parse().expect("headers");
        let resolver = MockTxt(txt.to_string());
        let config = Config::default();
        let mut verifier = Verifier::verify_header(&resolver, &header, &config)
            .await
            .expect("viadkim found a DKIM-Signature (filtermail 554 No DKIM signature found)");
        for chunk in body.as_bytes().chunks(8192) {
            if verifier.process_body_chunk(chunk) == BodyHasherStance::Done {
                break;
            }
        }
        let mut aligned_ok = false;
        for res in verifier.finish() {
            if matches!(res.status, VerificationStatus::Failure(_)) {
                panic!("viadkim verification failed: {:?}", res.status);
            }
            let Some(sig) = res.signature else {
                continue;
            };
            assert!(
                sig.domain.to_string().eq_ignore_ascii_case(from_domain),
                "d={} from={from_domain}",
                sig.domain
            );
            aligned_ok = true;
        }
        assert!(aligned_ok, "no aligned successful DKIM signature");
    }

    #[test]
    fn publish_info_creates_key_and_skips_ip() {
        let dir = tempfile::tempdir().unwrap();
        let info = publish_info(dir.path(), "mail.example.org").unwrap();
        assert_eq!(info["publishable"], true);
        assert_eq!(info["generated"], true);
        assert!(info["txt"].as_str().unwrap().starts_with("v=DKIM1;"));
        let again = publish_info(dir.path(), "mail.example.org").unwrap();
        assert_eq!(again["generated"], false);

        let ip = tempfile::tempdir().unwrap();
        let skip = publish_info(ip.path(), "203.0.113.10").unwrap();
        assert_eq!(skip["publishable"], false);
        assert!(skip["txt"].is_null());
        assert!(!private_key_path(ip.path(), "default").is_file());
    }

    #[test]
    fn inspect_info_does_not_create_key() {
        let dir = tempfile::tempdir().unwrap();
        let info = inspect_info(dir.path(), "mail.example.org").unwrap();
        assert_eq!(info["key_present"], false);
        assert_eq!(info["publishable"], false);
        assert!(!private_key_path(dir.path(), "default").is_file());
        assert!(info["reason"].as_str().unwrap().contains("no DKIM key"));
    }

    #[test]
    fn normalize_dkim_txt_ignores_quotes_and_whitespace() {
        let a = "v=DKIM1; k=rsa; p=ABCdef";
        let b = "\"v=DKIM1; \" \"k=rsa; p=ABCdef\"";
        assert_eq!(normalize_dkim_txt(a), normalize_dkim_txt(b));
        assert!(dkim_txt_matches(a, &[b.to_string()]));
        assert!(!dkim_txt_matches(a, &["v=DKIM1; k=rsa; p=XXXX".into()]));
    }

    #[tokio::test]
    async fn check_dns_with_mock_match_and_ip_skip() {
        let dir = tempfile::tempdir().unwrap();
        let info = publish_info(dir.path(), "mail.example.org").unwrap();
        let txt = info["txt"].as_str().unwrap().to_string();
        let ok = check_dns_with(dir.path(), "mail.example.org", |_fqdn| {
            let t = txt.clone();
            async move { Ok(vec![t]) }
        })
        .await
        .unwrap();
        assert_eq!(ok["matched"], true);
        assert_eq!(ok["checked"], true);

        let miss = check_dns_with(dir.path(), "mail.example.org", |_fqdn| async {
            Ok(vec!["v=DKIM1; k=rsa; p=nope".into()])
        })
        .await
        .unwrap();
        assert_eq!(miss["matched"], false);

        let ip = tempfile::tempdir().unwrap();
        let skip = check_dns_with(ip.path(), "127.0.0.1", |_fqdn| async {
            panic!("must not query DNS for IP From")
        })
        .await
        .unwrap();
        assert_eq!(skip["checked"], false);
        assert_eq!(skip["matched"], false);
    }

    #[test]
    fn signing_domain_rejects_ip() {
        assert_eq!(signing_domain("user@1.2.3.4"), None);
        assert_eq!(signing_domain("user@[1.2.3.4]"), None);
        assert_eq!(signing_domain("1.2.3.4"), None);
        assert_eq!(
            signing_domain("alice@d111-mm.madmail.chat"),
            Some("d111-mm.madmail.chat".into())
        );
    }

    #[test]
    fn load_or_create_writes_key_and_txt() {
        let dir = tempfile::tempdir().unwrap();
        let s = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        assert_eq!(s.domain, "mail.example.org");
        assert!(private_key_path(dir.path(), "default").is_file());
        let txt = fs::read_to_string(public_txt_path(dir.path(), "default")).unwrap();
        assert!(txt.starts_with("v=DKIM1; k=rsa; p="), "{txt}");
        let s2 = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        assert_eq!(
            s.public_txt(dir.path()).unwrap(),
            s2.public_txt(dir.path()).unwrap()
        );
    }

    #[tokio::test]
    async fn sign_is_verifiable_by_viadkim() {
        let dir = tempfile::tempdir().unwrap();
        let s = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        let raw = sample_msg("alice@mail.example.org");
        let signed = s.sign_message(&raw, "alice@mail.example.org").await;
        assert!(
            signed.starts_with(b"DKIM-Signature:"),
            "{}",
            String::from_utf8_lossy(&signed[..80.min(signed.len())])
        );
        assert!(signed.windows(raw.len()).any(|w| w == raw) || signed.len() > raw.len());
        let txt = s.public_txt(dir.path()).unwrap();
        viadkim_accepts(&signed, &txt, "mail.example.org").await;
    }

    #[tokio::test]
    async fn already_signed_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let s = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        let raw = sample_msg("alice@mail.example.org");
        let once = s.sign_message(&raw, "alice@mail.example.org").await;
        let twice = s.sign_message(&once, "alice@mail.example.org").await;
        assert_eq!(once, twice);
    }

    #[tokio::test]
    async fn ip_from_is_not_signed() {
        let dir = tempfile::tempdir().unwrap();
        let s = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        // cmdeploy treats IP From as a no-op; signing with our DNS d= would fail alignment.
        let raw = sample_msg("alice@1.2.3.4");
        let signed = s.sign_message(&raw, "alice@1.2.3.4").await;
        assert!(!signed.starts_with(b"DKIM-Signature:"));
        assert_eq!(signed, raw);
    }

    #[tokio::test]
    async fn foreign_from_is_not_signed() {
        let dir = tempfile::tempdir().unwrap();
        let s = DkimSigner::load_or_create(dir.path(), "default", "mail.example.org").unwrap();
        let raw = sample_msg("alice@other.example");
        let signed = s.sign_message(&raw, "alice@other.example").await;
        assert_eq!(signed, raw);
    }
}
