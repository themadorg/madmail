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

//! Server TLS from PEM files (`tls file` in maddy.conf).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use chatmail_types::{ChatmailError, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};

pub fn load_server_config(cert_path: &Path, key_path: &Path) -> Result<Arc<ServerConfig>> {
    let certs = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| ChatmailError::config(format!("TLS server config: {e}")))?;
    Ok(Arc::new(config))
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(path).map_err(|e| {
        ChatmailError::config(format!("open TLS certificate {}: {e}", path.display()))
    })?;
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    for item in certs(&mut reader) {
        let der = item.map_err(|e| ChatmailError::config(format!("parse TLS certificate: {e}")))?;
        out.push(der);
    }
    if out.is_empty() {
        return Err(ChatmailError::config(format!(
            "no certificates in {}",
            path.display()
        )));
    }
    Ok(out)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(path).map_err(|e| {
        ChatmailError::config(format!("open TLS private key {}: {e}", path.display()))
    })?;
    let mut reader = BufReader::new(file);
    private_key(&mut reader)
        .map_err(|e| {
            ChatmailError::config(format!("parse TLS private key {}: {e}", path.display()))
        })?
        .ok_or_else(|| {
            ChatmailError::config(format!(
                "no private key in {} (expected PKCS#8, PKCS#1 RSA, or SEC1 EC PEM)",
                path.display()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// OpenSSL `ecparam` / `-newkey ec` traditional form (`BEGIN EC PRIVATE KEY`).
    const EC_SEC1_KEY: &str = "-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIGL6GcbyVjjvOuVBu47P1AQ34r7B/rGjVn+I4cBTkyv2oAoGCCqGSM49\n\
AwEHoUQDQgAE0ek+2wDB9xWTkAhZBcScLJivmBPt5NzmfT8q775JxbWzNhSz5nfq\n\
CE5nnild39Ce+2fo1w76XqJz47/qojqh7A==\n\
-----END EC PRIVATE KEY-----\n";

    const EC_CERT: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBeDCCAR+gAwIBAgIUYQJVIT1R/TuS7Ev8bSZMa5OgAfEwCgYIKoZIzj0EAwIw\n\
EjEQMA4GA1UEAwwHZWMudGVzdDAeFw0yNjA4MjUxNjE1MzlaFw0zNjA4MjIxNjE1\n\
MzlaMBIxEDAOBgNVBAMMB2VjLnRlc3QwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNC\n\
AATR6T7bAMH3FZOQCFkFxJwsmK+YE+3k3OZ9PyrvvknFtbM2FLPmd+oITmeeKV3f\n\
0J77Z+jXDvpeonPjv+qiOqHso1MwUTAdBgNVHQ4EFgQUitXADSHqDFYPKxy16aeP\n\
yoNLuVMwHwYDVR0jBBgwFoAUitXADSHqDFYPKxy16aePyoNLuVMwDwYDVR0TAQH/\n\
BAUwAwEB/zAKBggqhkjOPQQDAgNHADBEAiAI5nAYmVQFXh4SZnqDbP2nltjmWqsl\n\
HPpOM0tP7Tv6BAIgFdCfiDj71XV2B2zi5Nfsikrksge3sbQ8UN51AAvzesA=\n\
-----END CERTIFICATE-----\n";

    const RSA_PKCS1_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----\n\
MIIEowIBAAKCAQEAkYVfxYC7zI4B3N9pjGr85MdIqP81viKx871zG8WJA70vtUJj\n\
7UTJsvg/xVUVXFH7Yl2/6/5Auo17EbHRkmjfiT3OpnGt0adAm1reP2+7xxBp9oXX\n\
fY5qHevahclSSXIjcFBdzJGnBYVO7/LZf6M9enJCg7JMdO990TDi7n77pEMxBO3X\n\
+8HfnP1fnW+5O1mfQTxIrbY2mnYA9LhGCkc7uVRvLyBR1bx8A0FyKKN8v3Iyupav\n\
o+hYFYRcNFsZ9OI3sYrgHB3wN3JhwO+0eA7V7Vlid1rcLF3+3jyPp8o2UFbkCuES\n\
uEe2kYdBJ+NVWanYgSU/BKk+EkzVapevDnhXAwIDAQABAoIBADQErEaKjRdDEBFn\n\
X3CNchdJ0YRvrkNoXZpWd4ZO53qJrzspH1VaiItMSGd+0aLtv2HbR1bRzUuidYLO\n\
wK6IhJenm25OJqdSFTszkUy14Tb4fBheobhFJ1PI0pWOcLbGcTqdz9nnmv/TNnN5\n\
qRwCO2DA5Vv0aXZHgf88bXJ5u/RscmHhUnA4v4i94eE8t78DN7Yurt1/VUe0O0Mv\n\
OPtcpNAAIxLge1JQNmM/KMzoGX0on830k1NS2xPDrZdMkQhjcMo6j41S7HYG2lZc\n\
wrEjf8bMklWBwrzK7I8wlkvTAeTQSBHrFrGpZA2P/rPa210ve+vjiabBkcAnVsqo\n\
/dvLi+kCgYEAw4FolziPZ3lu+qU15naPS8eQM2kg7n1/0eJUFe1MgtehCXCwH+2v\n\
9xHJDJDn+LIUZv2YIFsZZMRiIaiNcAPq2pAcCL4G/wD0BHjesX3iW12bOBjEEmME\n\
gbJvi9S/t7vtMMYGDae6ex136lPAxogXY7+rKpziyEDX1lROLYLp8JcCgYEAvoyJ\n\
+yt6EcDSNwovK0u8Ne8Hp9DCfc+OX1WXL1/qL9fNCypJh/PKSmY24rZrDUcYcGNw\n\
WGhEUatuSqSFEYNhJs1oOLjydRvs3u9HaI+bJ26pSts0te+fIF98kmRMOhDdb3NI\n\
91pxZFV5IEIPZkQYOB1ZzgG9/yt0l7vzi05H7nUCgYB5smVDtJ53n7x4YzzRD74V\n\
Qs09Y1RvgEl/ga4r1AILdGQ2tyG7Tj55wmVu4Ai140wV7Ae1JGADPMeFAiHAt3+K\n\
u6fnvTonpBVBb2fX/m9XxkXnvmrWszJL9aG/3hfVLDLyaGG+QEkxd998StQ2AOLm\n\
YZoPtYbpdoukS+g6JkKvUwKBgGYV1yqYXVq7iiPwsdqpRZlDiT9wCXLryuPqcAfy\n\
g/3DyNdtfV13z+3SGx+VCX9gkohLzfmfStLSXFFjGOOMFnV6YJbbBxKUtm+tk/1B\n\
yqbyk4JGNFQwn3jxj0TCtU/6jxfRlMroSo2teSo+Gg/49VzC5MUIi+j0OA++ozkD\n\
5GetAoGBALtPP2HiQRQWQBb07UUuDrQ2+iGMm9yryuKJ6Aopm+B2MWkKlKyzs9CX\n\
p7A91wW33eB953C7FwpsYQY/AQ6JdcExw+XqCV1GZkI0akl1vK14xCFarF9UH5+O\n\
T3gLGQ9RX6lgTT8YrxnfRMkV2Lol26Wuce/puVDMhiIXKLYqnZ/c\n\
-----END RSA PRIVATE KEY-----\n";

    const RSA_CERT: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDBzCCAe+gAwIBAgIUEobW2dCEL5ISxgaLvFZgg1RyK3IwDQYJKoZIhvcNAQEL\n\
BQAwEzERMA8GA1UEAwwIcnNhLnRlc3QwHhcNMjYwODI1MTYxNTQwWhcNMzYwODIy\n\
MTYxNTQwWjATMREwDwYDVQQDDAhyc2EudGVzdDCCASIwDQYJKoZIhvcNAQEBBQAD\n\
ggEPADCCAQoCggEBAJGFX8WAu8yOAdzfaYxq/OTHSKj/Nb4isfO9cxvFiQO9L7VC\n\
Y+1EybL4P8VVFVxR+2Jdv+v+QLqNexGx0ZJo34k9zqZxrdGnQJta3j9vu8cQafaF\n\
132Oah3r2oXJUklyI3BQXcyRpwWFTu/y2X+jPXpyQoOyTHTvfdEw4u5++6RDMQTt\n\
1/vB35z9X51vuTtZn0E8SK22Npp2APS4RgpHO7lUby8gUdW8fANBciijfL9yMrqW\n\
r6PoWBWEXDRbGfTiN7GK4Bwd8DdyYcDvtHgO1e1ZYnda3Cxd/t48j6fKNlBW5Arh\n\
ErhHtpGHQSfjVVmp2IElPwSpPhJM1WqXrw54VwMCAwEAAaNTMFEwHQYDVR0OBBYE\n\
FIUhRvPq09F5RZGfJQvO0+3XdyHdMB8GA1UdIwQYMBaAFIUhRvPq09F5RZGfJQvO\n\
0+3XdyHdMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEBAIvhr2z+\n\
OsedAQpyni3uKthldo48A1quZJFZaOw9zdWcyPfX/6vbq08VXGhmCAYxsQLMcUdT\n\
PruNxnDGw3vhWXHY1333vZ97wJO4BAYqwS9iuCjRfLfpeskJPYcQEl/6pvjciwhl\n\
MKiIE5xU8T7AujUegw8KphoV3Xc/+2raeRxjJT0n4ipS0WniTcvvEcUwqTqtGzYV\n\
VYqtDttoAtOQMGDfCo6yuGQ5SY11xVaC0pc9vlZdtWOoYdbtRNzvb6HinvhUSFjf\n\
uYqvPfQ1h65fbVyoAel0qh88mghAcKN9QwKpO2Cg2IvNrRApO5nuOPaabUgmg86d\n\
BCsnj4xR6GA3R0A=\n\
-----END CERTIFICATE-----\n";

    fn write_pair(
        cert_pem: &str,
        key_pem: &str,
    ) -> (tempfile::NamedTempFile, tempfile::NamedTempFile) {
        let mut cert = tempfile::NamedTempFile::new().unwrap();
        let mut key = tempfile::NamedTempFile::new().unwrap();
        cert.write_all(cert_pem.as_bytes()).unwrap();
        key.write_all(key_pem.as_bytes()).unwrap();
        cert.flush().unwrap();
        key.flush().unwrap();
        (cert, key)
    }

    fn init_crypto() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn loads_sec1_ecdsa_p256() {
        init_crypto();
        let (cert, key) = write_pair(EC_CERT, EC_SEC1_KEY);
        load_server_config(cert.path(), key.path()).expect("SEC1 EC key must load");
    }

    #[test]
    fn loads_rsa_pkcs1() {
        init_crypto();
        let (cert, key) = write_pair(RSA_CERT, RSA_PKCS1_KEY);
        load_server_config(cert.path(), key.path()).expect("PKCS#1 RSA key must load");
    }

    #[test]
    fn rejects_empty_key_file() {
        let (cert, key) = write_pair(EC_CERT, "");
        let err = load_server_config(cert.path(), key.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no private key"), "{msg}");
        assert!(msg.contains("SEC1"), "{msg}");
    }
}
