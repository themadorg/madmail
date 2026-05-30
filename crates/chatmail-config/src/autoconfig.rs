//! Mozilla ISPDB autoconfig (`/.well-known/autoconfig/mail/config-v1.1.xml`).

use crate::{port_from_listen, DcloginMailSettings, RuntimeListeners};

/// Inputs for generating Mozilla mail autoconfig XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoconfigParams {
    /// `<domain>` and provider id (e.g. `example.org` or `[192.0.2.1]`).
    pub mail_domain: String,
    /// Hostname clients connect to for IMAP/SMTP (`ih` / `sh`).
    pub client_host: String,
    pub imap_port_tls: String,
    pub imap_port_starttls: String,
    pub smtp_port_tls: String,
    pub smtp_port_starttls: String,
    /// Bound listeners — only advertise servers that exist.
    pub has_imap_tls: bool,
    pub has_imap_plain: bool,
    pub has_submission_tls: bool,
    pub has_submission_plain: bool,
    /// `chatmail tls://443 { alpn_imap imap }` — IMAP over HTTPS ALPN.
    pub has_imap_alpn_https: bool,
    pub https_port: Option<String>,
}

impl AutoconfigParams {
    pub fn from_mail_settings(
        mail_domain: &str,
        mail: &DcloginMailSettings,
        runtime: Option<&RuntimeListeners>,
    ) -> Self {
        let has_imap_tls = listener_bound(runtime, |r| r.imap_tls_addr.as_deref())
            || mail.dclogin_imap_security == "ssl";
        let has_imap_plain = listener_bound(runtime, |r| r.imap_plain_addr.as_deref())
            || mail.dclogin_imap_security == "plain"
            || mail.dclogin_imap_security == "starttls";
        let has_submission_tls = listener_bound(runtime, |r| r.submission_tls_addr.as_deref())
            || mail.dclogin_smtp_security == "ssl";
        let has_submission_plain = listener_bound(runtime, |r| r.submission_plain_addr.as_deref())
            || mail.dclogin_smtp_security == "plain"
            || mail.dclogin_smtp_security == "starttls"
            || mail.dclogin_smtp_security == "default";

        let https_port = runtime
            .and_then(|r| r.http_tls_addr.as_deref())
            .and_then(|addr| port_from_listen(Some(addr)));

        Self {
            mail_domain: mail_domain.to_string(),
            client_host: mail.client_host.clone(),
            imap_port_tls: mail.imap_port_tls.clone(),
            imap_port_starttls: mail.imap_port_starttls.clone(),
            smtp_port_tls: mail.smtp_port_tls.clone(),
            smtp_port_starttls: mail.smtp_port_starttls.clone(),
            has_imap_tls,
            has_imap_plain,
            has_submission_tls,
            has_submission_plain,
            // chatmail-fed does not implement IMAP-over-HTTPS ALPN on 443 yet.
            has_imap_alpn_https: false,
            https_port,
        }
    }
}

fn listener_bound<F>(runtime: Option<&RuntimeListeners>, f: F) -> bool
where
    F: FnOnce(&RuntimeListeners) -> Option<&str>,
{
    runtime.and_then(f).is_some()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn incoming_server(host: &str, port: &str, socket_type: &str) -> String {
    format!(
        r#"    <incomingServer type="imap">
      <hostname>{host}</hostname>
      <port>{port}</port>
      <socketType>{socket_type}</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </incomingServer>
"#
    )
}

fn outgoing_server(host: &str, port: &str, socket_type: &str) -> String {
    format!(
        r#"    <outgoingServer type="smtp">
      <hostname>{host}</hostname>
      <port>{port}</port>
      <socketType>{socket_type}</socketType>
      <authentication>password-cleartext</authentication>
      <username>%EMAILADDRESS%</username>
    </outgoingServer>
"#
    )
}

/// Build Mozilla autoconfig XML (cmdeploy / Thunderbird ISPDB format).
pub fn build_autoconfig_xml(params: &AutoconfigParams) -> String {
    let id = xml_escape(&params.mail_domain);
    let domain = xml_escape(strip_brackets(&params.mail_domain));
    let host = xml_escape(&params.client_host);
    let display = xml_escape(&strip_brackets(&params.mail_domain));

    let mut incoming = String::new();
    if params.has_imap_tls {
        incoming.push_str(&incoming_server(
            &host,
            &params.imap_port_tls,
            "SSL",
        ));
    }
    if params.has_imap_plain {
        incoming.push_str(&incoming_server(
            &host,
            &params.imap_port_starttls,
            "STARTTLS",
        ));
    }
    if params.has_imap_alpn_https {
        if let Some(ref port) = params.https_port {
            incoming.push_str(&incoming_server(&host, port, "SSL"));
        }
    }

    let mut outgoing = String::new();
    if params.has_submission_tls {
        outgoing.push_str(&outgoing_server(
            &host,
            &params.smtp_port_tls,
            "SSL",
        ));
    }
    if params.has_submission_plain {
        outgoing.push_str(&outgoing_server(
            &host,
            &params.smtp_port_starttls,
            "STARTTLS",
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<clientConfig version="1.1">
  <emailProvider id="{id}">
    <domain>{domain}</domain>
    <displayName>{display} chatmail</displayName>
    <displayShortName>{display}</displayShortName>
{incoming}{outgoing}  </emailProvider>
</clientConfig>
"#
    )
}

fn strip_brackets(s: &str) -> &str {
    s.trim_matches(['[', ']'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppConfig;

    #[test]
    fn autoconfig_includes_ssl_and_starttls_when_both_listeners() {
        let cfg = AppConfig {
            imap_listen: Some("0.0.0.0:143".into()),
            imap_tls_listen: Some("0.0.0.0:993".into()),
            submission_listen: Some("0.0.0.0:587".into()),
            submission_tls_listen: Some("0.0.0.0:465".into()),
            mail_domain: Some("example.org".into()),
            ..Default::default()
        };
        let mail = DcloginMailSettings::from_config(&cfg, None);
        let rt = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:143".into()),
            imap_tls_addr: Some("0.0.0.0:993".into()),
            submission_plain_addr: Some("0.0.0.0:587".into()),
            submission_tls_addr: Some("0.0.0.0:465".into()),
            smtp_addr: Some("0.0.0.0:25".into()),
            http_plain_addr: None,
            http_tls_addr: None,
        };
        let params = AutoconfigParams::from_mail_settings("example.org", &mail, Some(&rt));
        let xml = build_autoconfig_xml(&params);
        assert!(xml.contains("<port>993</port>"));
        assert!(xml.contains("<port>143</port>"));
        assert!(xml.contains("<socketType>SSL</socketType>"));
        assert!(xml.contains("<socketType>STARTTLS</socketType>"));
        assert!(xml.contains("<port>465</port>"));
        assert!(xml.contains("<port>587</port>"));
        assert!(xml.contains("<domain>example.org</domain>"));
    }

    #[test]
    fn autoconfig_ip_domain_strips_brackets_in_domain_tag() {
        let mail = DcloginMailSettings {
            client_host: "192.0.2.1".into(),
            imap_port_tls: "993".into(),
            imap_port_starttls: "143".into(),
            smtp_port_tls: "465".into(),
            smtp_port_starttls: "587".into(),
            dclogin_imap_security: "ssl".into(),
            dclogin_smtp_security: "ssl".into(),
        };
        let rt = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:143".into()),
            imap_tls_addr: Some("0.0.0.0:993".into()),
            submission_plain_addr: Some("0.0.0.0:587".into()),
            submission_tls_addr: Some("0.0.0.0:465".into()),
            smtp_addr: None,
            http_plain_addr: None,
            http_tls_addr: None,
        };
        let params =
            AutoconfigParams::from_mail_settings("[192.0.2.1]", &mail, Some(&rt));
        let xml = build_autoconfig_xml(&params);
        assert!(xml.contains("<domain>192.0.2.1</domain>"));
        assert!(xml.contains("<hostname>192.0.2.1</hostname>"));
    }

    #[test]
    fn autoconfig_omits_https_alpn_even_when_http_tls_bound() {
        let mail = DcloginMailSettings {
            client_host: "example.org".into(),
            imap_port_tls: "993".into(),
            imap_port_starttls: "143".into(),
            smtp_port_tls: "465".into(),
            smtp_port_starttls: "587".into(),
            dclogin_imap_security: "ssl".into(),
            dclogin_smtp_security: "ssl".into(),
        };
        let rt = RuntimeListeners {
            imap_plain_addr: Some("0.0.0.0:143".into()),
            imap_tls_addr: Some("0.0.0.0:993".into()),
            submission_plain_addr: Some("0.0.0.0:587".into()),
            submission_tls_addr: Some("0.0.0.0:465".into()),
            smtp_addr: None,
            http_plain_addr: None,
            http_tls_addr: Some("0.0.0.0:443".into()),
        };
        let params = AutoconfigParams::from_mail_settings("example.org", &mail, Some(&rt));
        assert!(!params.has_imap_alpn_https);
        let xml = build_autoconfig_xml(&params);
        assert!(!xml.contains("<port>443</port>"));
        assert_eq!(xml.matches("<incomingServer").count(), 2);
    }
}
