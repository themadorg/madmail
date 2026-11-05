# Kubernetes deployment guide (Helm)

The Helm chart in [`deploy/helm/madmail`](../../deploy/helm/madmail/) runs Madmail on Kubernetes with the same image, paths and `madmail install` bootstrap as the [Docker guide](docker.md).

## Table of contents

- [How the chart works](#how-the-chart-works)
- [Requirements](#requirements)
- [Quick start (IP relay)](#quick-start-ip-relay)
- [Domain with Let's Encrypt](#domain-with-lets-encrypt)
- [Domain with cert-manager](#domain-with-cert-manager)
- [Networking](#networking)
  - [LoadBalancer (default)](#loadbalancer-default)
  - [Host networking (recommended for calls)](#host-networking-recommended-for-calls)
  - [TURN relay ports](#turn-relay-ports)
- [Admin UI, metrics and Iroh](#admin-ui-metrics-and-iroh)
- [Day-2 operations](#day-2-operations)
- [Values reference](#values-reference)
- [Limitations](#limitations)

## How the chart works

| Object | Purpose |
|--------|---------|
| `StatefulSet` (1 replica) | Madmail keeps its state in SQLite and binds mail ports itself, so there is exactly one pod. Do not scale it. |
| init container `bootstrap` | First start: runs `madmail install --simple …` and writes `/etc/madmail/madmail.conf`, TLS PEMs, DKIM keys and the databases. Every start: applies `adminWeb` and `metrics` settings. |
| PVC `state` → `/var/lib/madmail` | SQLite databases, maildirs, DKIM keys, `admin_token`. |
| PVC `config` → `/etc/madmail` | `madmail.conf` and `certs/` (writable: ACME renewal rewrites the PEMs). |
| `Service` (LoadBalancer) | SMTP, submission, IMAP, HTTP/HTTPS, TURN, Shadowsocks, Iroh. `externalTrafficPolicy: Local` keeps client IPs. |
| `Service` `-headless` | StatefulSet identity; metrics port for the ServiceMonitor. |

The bootstrap only installs when `madmail.conf` is missing from the config volume, so restarts and upgrades never overwrite your configuration.

## Requirements

- Kubernetes **1.26+** (mixed TCP/UDP LoadBalancer Services).
- A **static public IP** you can assign to the LoadBalancer (or a node with a public IP for `hostNetwork`). Madmail advertises this address for TURN and in client setup, so it must be known before the first install.
- Inbound TCP 25 from the internet if you want mail from other servers. Many clouds block it by default.
- A `ReadWriteOnce` storage class.

## Quick start (IP relay)

Self-signed TLS, addresses like `user@[203.0.113.50]`:

```bash
helm install madmail ./deploy/helm/madmail \
  --namespace madmail --create-namespace \
  --set madmail.publicIp=203.0.113.50 \
  --set service.loadBalancerIP=203.0.113.50

kubectl -n madmail logs madmail-0 -c bootstrap -f
kubectl -n madmail get svc madmail -w           # EXTERNAL-IP must be 203.0.113.50
kubectl -n madmail exec madmail-0 -- madmail admin-token
helm test -n madmail madmail
```

Open `https://203.0.113.50/` to register with Delta Chat.

`service.loadBalancerIP` works on most providers. Some need an annotation instead (for example `service.beta.kubernetes.io/azure-load-balancer-ipv4` or `metallb.universe.tf/loadBalancerIPs`); put it under `service.annotations`.

## Domain with Let's Encrypt

```yaml
# values-domain.yaml
madmail:
  publicIp: 203.0.113.50
  domain: example.org
  hostname: example.org
  tls:
    mode: autocert
    acmeEmail: admin@example.org
service:
  loadBalancerIP: 203.0.113.50
```

```bash
helm install madmail ./deploy/helm/madmail -n madmail --create-namespace -f values-domain.yaml
```

Before installing, point `A`/`AAAA` for the hostname at the IP and `MX` for the domain at the hostname (see [DNS checklist](docker.md#dns-checklist-domain-production)). The first install answers the HTTP-01 challenge on port 80 from the bootstrap init container. With `tls.mode: autocert` the Service sets `publishNotReadyAddresses: true` so the challenge reaches the pod before it is Ready. If issuance fails, the init container exits without writing `madmail.conf` and Kubernetes retries it; read the log with `kubectl logs madmail-0 -c bootstrap`.

Madmail renews the certificate itself on port 80, so keep port 80 published.

## Domain with cert-manager

Let cert-manager own the certificate and mount it with `tls.mode: file`:

```yaml
# certificate.yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: madmail-tls
  namespace: madmail
spec:
  secretName: madmail-tls
  dnsNames: [example.org]
  privateKey:
    algorithm: ECDSA
    size: 256
  issuerRef:
    kind: ClusterIssuer
    name: letsencrypt-dns   # use a DNS-01 issuer; HTTP-01 would need port 80 routed to an ingress
```

```yaml
# values-certmanager.yaml
madmail:
  publicIp: 203.0.113.50
  domain: example.org
  tls:
    mode: file
    existingSecret: madmail-tls
service:
  loadBalancerIP: 203.0.113.50
```

The Secret is mounted read-only at `/etc/madmail/certs` as `fullchain.pem` / `privkey.pem`. Madmail reads PEMs at start, so restart the pod after cert-manager renews (`kubectl -n madmail rollout restart statefulset madmail`, or automate it with a tool such as [Reloader](https://github.com/stakater/Reloader) via `podAnnotations`).

## Networking

### LoadBalancer (default)

One Service publishes every port. `externalTrafficPolicy: Local` preserves client source IPs, which Madmail needs for rate limits, SPF checks and bans. If your load balancer cannot mix TCP and UDP on one Service, set `service.separateUdpService: true`; TURN then moves to a second Service named `<release>-madmail-udp`. Whether both Services can share one IP depends on the provider (MetalLB: `metallb.universe.tf/allow-shared-ip` on both).

### Host networking (recommended for calls)

```yaml
hostNetwork: true
madmail:
  publicIp: 198.51.100.7          # the node's public IP
nodeSelector:
  kubernetes.io/hostname: mail-node-1
```

Madmail binds directly on the node, so the full TURN relay range (`49152–65535`) works without listing ports, and clients see real IPs. The Service becomes `ClusterIP` (in-cluster access only). Open the ports in the node and cloud firewall ([port list](docker.md#ports)), and pin the pod to one node so its IP stays stable.

### TURN relay ports

A Kubernetes Service cannot publish a port range, so without `hostNetwork` the chart lists every relay port on the Service and writes the same range into `madmail.conf` at first install (`relay_port_min` / `relay_port_max` in the `turn` block). The default is `50000–50019` (20 concurrent call legs). Cloud load balancers cap listener counts (AWS NLB: 50 by default), so raise `madmail.turn.relayPortMax` carefully. The chart refuses ranges over 1000 ports.

The range is written only at first install. To change it later, edit both `madmail.conf` (or the admin **Services** page) and the chart values, then upgrade.

## Admin UI, metrics and Iroh

These settings are applied on **every** pod start, so you can toggle them with `helm upgrade`:

```yaml
madmail:
  adminWeb:
    enabled: true
    path: /admin-7f3c          # hard-to-guess path on public servers
  metrics:
    enabled: true              # madmail monitor enable --ip 0.0.0.0
    port: 9749
    existingSecret: madmail-metrics   # optional: keys `username`, `password`
    serviceMonitor:
      enabled: true            # needs the Prometheus Operator CRDs
networkPolicy:
  enabled: true
  metricsFrom:
    - namespaceSelector:
        matchLabels:
          kubernetes.io/metadata.name: monitoring
```

Metrics are served on the headless Service, not the public LoadBalancer. Without a password the endpoint is unauthenticated; enable `networkPolicy` or set `existingSecret`.

`madmail.iroh.enabled: true` passes `--enable-iroh` at first install and publishes TCP 3340. On an existing install run `kubectl exec madmail-0 -- madmail iroh install`, set the value, and `helm upgrade`.

## Day-2 operations

| Task | Command |
|------|---------|
| Logs | `kubectl -n madmail logs -f madmail-0` |
| Bootstrap log | `kubectl -n madmail logs madmail-0 -c bootstrap` |
| CLI | `kubectl -n madmail exec madmail-0 -- madmail status` |
| Admin token | `kubectl -n madmail exec madmail-0 -- madmail admin-token` |
| DKIM record | `kubectl -n madmail exec madmail-0 -- madmail dkim show` |
| Restart | `kubectl -n madmail rollout restart statefulset madmail` |
| Edit config | `kubectl -n madmail exec -it madmail-0 -- vi /etc/madmail/madmail.conf`, then restart |
| Upgrade | `helm upgrade madmail ./deploy/helm/madmail -n madmail --reuse-values --set image.tag=X.Y.Z` |
| Backup | Snapshot both PVCs (`state-madmail-0`, `config-madmail-0`) with your CSI driver, or `kubectl cp madmail/madmail-0:/var/lib/madmail ./backup/lib` (and `/etc/madmail`) |
| Re-run install | Delete `/etc/madmail/madmail.conf` on the config volume and restart. Existing users and mail in `/var/lib/madmail` stay. |

`madmail.publicIp`, `domain`, `hostname`, `lang`, `tls.*`, `turn.*`, `iroh.enabled` and `installExtraArgs` are install-time values. Changing them later in Helm has no effect on a running server; use the CLI or edit `madmail.conf`. PVCs are kept on `helm uninstall` (`persistentVolumeClaimRetentionPolicy: Retain`); delete them yourself to wipe the server.

## Values reference

See [`values.yaml`](../../deploy/helm/madmail/values.yaml) for every option with comments. The most common:

| Value | Default | Description |
|-------|---------|-------------|
| `madmail.publicIp` | — | **Required.** Public IP for TURN, Iroh, client setup. |
| `madmail.domain` / `madmail.hostname` | `""` | Mail domain and TLS hostname. Empty domain = IP relay. |
| `madmail.tls.mode` | `""` | `autocert`, `self_signed`, `file` or install default. |
| `madmail.tls.existingSecret` | `""` | `kubernetes.io/tls` Secret for `mode: file`. |
| `madmail.turn.relayPortMin/Max` | `50000`/`50019` | Published TURN relay range (ignored with `hostNetwork`). |
| `madmail.adminWeb.enabled` | `false` | Embedded admin UI. |
| `madmail.metrics.enabled` | `false` | Prometheus endpoint on the headless Service. |
| `madmail.installExtraArgs` | `[]` | Extra `madmail install` flags (first run). |
| `hostNetwork` | `false` | Bind on the node network. |
| `service.type` | `LoadBalancer` | `ClusterIP` when `hostNetwork` is on. |
| `service.loadBalancerIP` | `""` | Static IP, normally equal to `madmail.publicIp`. |
| `service.separateUdpService` | `false` | Put TURN on its own Service. |
| `persistence.state.size` / `persistence.config.size` | `10Gi` / `1Gi` | PVC sizes; `existingClaim` reuses a PVC. |
| `image.tag` | chart `appVersion` | Madmail image version. |

## Limitations

- One replica only; there is no high-availability mode.
- No Ingress: Madmail terminates TLS itself and shares the certificate between HTTPS, IMAP and SMTP. Route TCP 80/443 straight to the Service.
- With the default LoadBalancer setup, TURN capacity is bounded by the number of published relay ports.
- The image runs as root with every capability dropped except `NET_BIND_SERVICE`; the main container has a read-only root filesystem, the bootstrap container does not (`install` writes the binary, man pages and completions there).
