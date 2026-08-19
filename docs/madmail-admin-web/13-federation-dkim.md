# Federation — DKIM (`/federation/dkim`)

**Sources:** `src/routes/federation/+layout.svelte`, `src/routes/federation/dkim/+page.svelte`

## Purpose

Show the outbound federation **DKIM** record so the operator can publish a TXT at `default._domainkey`. Same payload as `madmail dkim show`.

Visiting this page calls `GET /admin/dkim`, which creates `{state_dir}/dkim/default.private` when missing and the mail domain is a DNS name.

## Data loaded

| When | Loader | API |
|------|--------|-----|
| Prefetch on this route | `loadDkim()` | `GET /admin/dkim` |
| Check DNS | `api.dkimCheck()` | `GET /admin/dkim/check` |
| Federation layout | `loadFederationSection()` | (policy + peers) |

## Fields

| Field | Meaning |
|-------|---------|
| `selector` | Always `default` |
| `domain` | Signing domain (`d=`) |
| `dns_name` | Relative record name (`default._domainkey`) |
| `dns_fqdn` | Absolute name to publish, or `null` for IP-literal domains |
| `txt` | TXT value (`v=DKIM1; k=rsa; p=…`), or `null` when not publishable |
| `publishable` | `true` when a DNS `d=` exists and the key is on disk |
| `generated` | `true` when this request created the key |
| `reason` | Why the record is not publishable (IP-literal domains) |
| `private_key_path` / `txt_path` | Paths on the server |

## Actions

Copy buttons for FQDN, TXT, and a BIND-style zone line. **Check DNS** calls `GET /admin/dkim/check` (`madmail dkim check`) and shows whether the published TXT matches.

No other mutations besides the GET side-effect that creates a missing key.

IP-literal mail domains show `publishable: false` and the server `reason` (no key is written).

Servers without `/admin/dkim` (404 / unknown resource) show an unsupported empty state.

## Typical usage

- Open **Federation → DKIM** after install or upgrade and copy the TXT before the first federated send
- Publish `default._domainkey.<domain>` so cmdeploy `filtermail` can verify the signature
- Use **Check DNS** (or `madmail dkim check`) after the TXT is live
- Same-stack Madmail↔Madmail does not require the TXT; other stacks do
