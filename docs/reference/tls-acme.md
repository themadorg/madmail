# Automatic certificate management via ACME

Maddy supports obtaining certificates using ACME protocol.

To use it, create a configuration name for `tls.loader.acme`
and reference it from endpoints that should use automatically
configured certificates:

```
tls.loader.acme local_tls {
    email put-your-email-here@example.org
    agreed # indicate your agreement with Let's Encrypt ToS
    challenge dns-01
}

smtp tcp://127.0.0.1:25 {
    tls &local_tls
    ...
}
```

You can also use a global `tls` directive to use automatically
obtained certificates for all endpoints:

```
tls {
    loader acme {
        email maddy-acme@example.org
        agreed
        challenge dns-01
    }
}
```

Note: `tls &local_tls` as a global directive won't work because
global directives are initialized before other configuration blocks.

Currently the only supported challenge is `dns-01` one therefore
you also need to configure the DNS provider:

```
tls.loader.acme local_tls {
    email maddy-acme@example.org
    agreed
    challenge dns-01
    dns PROVIDER_NAME {
        ...
    }
}
```

See below for supported providers and necessary configuration
for each.

## Configuration directives

```
tls.loader.acme {
    debug off
    hostname example.maddy.invalid
    store_path /var/lib/maddy/acme
    ca https://acme-v02.api.letsencrypt.org/directory
    test_ca https://acme-staging-v02.api.letsencrypt.org/directory
    email test@maddy.invalid
    agreed off
    challenge dns-01
    dns ...
}
```

### debug _boolean_
Default: global directive value

Enable debug logging.

---

### hostname _str_
**Required.**<br>
Default: global directive value

Domain name to issue certificate for.

---

### store_path _path_
Default: `state_dir/acme`

Where to store issued certificates and associated metadata.
Currently only filesystem-based store is supported.

---

### ca _url_
Default: Let's Encrypt production CA

URL of ACME directory to use.

---

### test_ca _url_
Default: Let's Encrypt staging CA

URL of ACME directory to use for retries should
primary CA fail.

maddy will keep attempting to issues certificates
using `test_ca` until it succeeds then it will switch
back to the one configured via 'ca' option.

This avoids rate limit issues with production CA.

---

### override_domain _domain_
Default: not set

Override the domain to set the TXT record on for DNS-01 challenge.
This is to delegate the challenge to a different domain.

See https://www.eff.org/deeplinks/2018/02/technical-deep-dive-securing-automation-acme-dns-challenge-validation
for explanation why this might be useful.

---

### email _str_
Default: not set

Email to pass while registering an ACME account.

---

### agreed _boolean_
Default: false

Whether you agreed to ToS of the CA service you are using.

---

### challenge `dns-01`
Default: not set

Challenge(s) to use while performing domain verification.
