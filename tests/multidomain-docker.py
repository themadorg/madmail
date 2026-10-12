"""Exercise browser registration and live TLS authentication in the Docker relay."""

import http.client
import imaplib
import json
import secrets
import smtplib
import ssl


# The isolated test relay uses a self-signed loopback certificate.
tls = ssl._create_unverified_context()


def request(method, path, host):
    conn = http.client.HTTPSConnection("127.0.0.1", 443, context=tls, timeout=10)
    conn.putrequest(method, path, skip_host=True)
    conn.putheader("Host", host)
    conn.putheader("Content-Length", "0")
    conn.endheaders()
    response = conn.getresponse()
    body = response.read()
    assert response.status == 200, (host, path, response.status)
    conn.close()
    return body


def login(email, password):
    with imaplib.IMAP4_SSL("127.0.0.1", 993, ssl_context=tls, timeout=10) as imap:
        assert imap.login(email, password)[0] == "OK"
    smtp_login(email, password)


def smtp_login(email, password):
    with smtplib.SMTP_SSL("127.0.0.1", 465, context=tls, timeout=10) as smtp:
        smtp.login(email, password)


def reject_login(email, password):
    try:
        login(email, password)
    except imaplib.IMAP4.error:
        pass
    else:
        raise AssertionError("Unexpected IMAP login success")
    try:
        smtp_login(email, password)
    except smtplib.SMTPAuthenticationError:
        pass
    else:
        raise AssertionError("Unexpected SMTP login success")


for host, domain in [
    ("127.0.0.1", "[127.0.0.1]"),
    ("b.com", "b.com"),
    ("C.COM:443", "c.com"),
    ("foreign.example", "[127.0.0.1]"),
    ("b.com.attacker.example", "[127.0.0.1]"),
]:
    html = request("GET", "/", host).decode()
    assert f'const REGISTRATION_DOMAIN = "{domain.strip("[]")}"' in html, host
    account = json.loads(request("POST", "/new", host))
    email, password = account["email"], account["password"]
    assert email.endswith("@" + domain), host
    login(email, password)
    reject_login(email, "incorrect-password")

# The browser page generates credentials locally; first protocol login creates them.
for domain in ["b.com", "c.com", "[127.0.0.1]"]:
    login(secrets.token_hex(6) + "@" + domain, secrets.token_hex(12))

for domain in ["foreign.example", "b.com.attacker.example"]:
    reject_login(secrets.token_hex(6) + "@" + domain, secrets.token_hex(12))

# Identical localparts on different domains remain independent accounts.
localpart = secrets.token_hex(6)
first_password, second_password = secrets.token_hex(12), secrets.token_hex(12)
login(localpart + "@b.com", first_password)
login(localpart + "@c.com", second_password)
reject_login(localpart + "@b.com", second_password)
reject_login(localpart + "@c.com", first_password)

print("PASS: configured hosts, foreign-host fallback, JIT allowlist, password and account isolation")
