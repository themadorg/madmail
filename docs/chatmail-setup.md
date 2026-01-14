# Chatmail Server Setup Guide

This guide explains how to set up a chatmail server using the Maddy Chatmail fork. Unlike traditional email servers, chatmail servers are optimized for secure, instant messaging with automatic encryption and minimal data retention.

## What is Chatmail?

Chatmail servers are email servers optimized for secure messaging rather than traditional email. They prioritize:

- **Instant account creation** without personal information
- **Encryption-only messaging** to ensure privacy
- **Automatic cleanup** to minimize data retention
- **Low maintenance** for easy deployment

Chatmail servers work seamlessly with [Delta Chat](https://delta.chat), providing a secure, decentralized messaging experience.

## Community
Join our Telegram channel for updates and support: **[https://t.me/the_madmail](https://t.me/the_madmail)**

## Prerequisites

Before setting up your chatmail server, ensure you have:

- A server with a public IP address
- Root or sudo access
- A domain name pointing to your server
- Basic knowledge of Linux system administration

## Quick Start with Docker

The fastest way to get started is using Docker:

```bash
# Create a volume for persistent data
docker volume create maddy-data

# Run the chatmail server
docker run -d \
  --name maddy-chatmail \
  -e MADDY_HOSTNAME=mail.yourdomain.com \
  -e MADDY_DOMAIN=yourdomain.com \
  -v maddy-data:/data \
  -p 25:25 \
  -p 143:143 \
  -p 465:465 \
  -p 587:587 \
  -p 993:993 \
  -p 80:80 \
  -p 443:443 \
  ghcr.io/themadorg/madmail:latest

# Copy TLS certificates to the container
docker cp /path/to/fullchain.pem maddy-chatmail:/data/tls/fullchain.pem
docker cp /path/to/privkey.pem maddy-chatmail:/data/tls/privkey.pem

# Restart the container
docker restart maddy-chatmail
```

### Docker Compose

Alternatively, you can use Docker Compose for easier management. Create a `docker-compose.yml` file:

```yaml
version: '3.8'

services:
  maddy-chatmail:
    image: ghcr.io/themadorg/madmail:latest
    environment:
      - MADDY_HOSTNAME=mail.yourdomain.com
      - MADDY_DOMAIN=yourdomain.com
    volumes:
      - maddy-data:/data
      - ./certs:/data/tls:ro  # Mount certificates if available
    ports:
      - "25:25"
      - "143:143"
      - "465:465"
      - "587:587"
      - "993:993"
      - "80:80"
      - "443:443"
    restart: unless-stopped

volumes:
  maddy-data:
```

Then run:

```bash
docker-compose up -d
```

If certificates are not mounted, copy them to the container as described above.

## Manual Installation

### 1. Install Maddy Chatmail

Download the latest release from the [GitHub releases page](https://github.com/themadorg/madmail/releases):

```bash
# Download and extract
wget https://github.com/themadorg/madmail/releases/download/v0.8.3/maddy-linux-amd64.tar.gz
tar -xzf maddy-linux-amd64.tar.gz
sudo mv maddy /usr/local/bin/

# Create maddy user
sudo useradd -mrU -s /sbin/nologin -d /var/lib/maddy -c "maddy mail server" maddy
```

### 2. Create Directories

```bash
sudo mkdir -p /etc/maddy/certs/{yourdomain.com}
sudo mkdir -p /var/lib/maddy
sudo chown -R maddy:maddy /var/lib/maddy
```

### 3. Configure Maddy

Create `/etc/maddy/maddy.conf`:

```maddy
## Maddy Chatmail Configuration

$(hostname) = mail.yourdomain.com
$(primary_domain) = yourdomain.com
$(local_domains) = $(primary_domain)

# TLS certificates
tls file /etc/maddy/certs/yourdomain.com/fullchain.pem /etc/maddy/certs/yourdomain.com/privkey.pem

# Authentication database
auth.pass_table local_authdb {
    table sql_table {
        driver sqlite3
        dsn /var/lib/maddy/credentials.db
        table_name passwords
    }
}

# Message storage
storage.imapsql local_mailboxes {
    driver sqlite3
    dsn /var/lib/maddy/imapsql.db
}

# SMTP endpoints
hostname $(hostname)

table.chain local_rewrites {
    optional_step regexp "(.+)\+(.+)@(.+)" "$1@$3"
    optional_step static {
        entry postmaster postmaster@$(primary_domain)
    }
    optional_step file /etc/maddy/aliases
}

msgpipeline local_routing {
    destination postmaster $(local_domains) {
        modify {
            replace_rcpt &local_rewrites
        }
        deliver_to &local_mailboxes
    }
    default_destination {
        reject 550 5.1.1 "User doesn't exist"
    }
}

# SMTP ports
smtp tcp://0.0.0.0:25 {
    limits {
        all rate 20 1s
        all concurrency 10
    }
    dmarc yes
    check {
        require_mx_record
        dkim
        spf
    }
    source $(local_domains) {
        reject 501 5.1.8 "Use Submission for outgoing SMTP"
    }
}

submission tls://0.0.0.0:465 tcp://0.0.0.0:587 {
    limits {
        all rate 50 1s
    }
    auth &local_authdb
    source $(local_domains) {
        check {
            authorize_sender {
                prepare_email &local_rewrites
                user_to_email identity
            }
        }
        destination postmaster $(local_domains) {
            deliver_to &local_routing
        }
        default_destination {
            modify {
                dkim $(primary_domain) $(local_domains) default
            }
            deliver_to &remote_queue
        }
    }
}

# Outbound delivery
target.remote outbound_delivery {
    limits {
        destination rate 20 1s
        destination concurrency 10
    }
    mx_auth {
        dane
        mtasts {
            cache fs
            fs_dir /var/lib/maddy/mtasts_cache/
        }
        local_policy {
            min_tls_level encrypted
            min_mx_level none
        }
    }
}

target.queue remote_queue {
    target &outbound_delivery
    autogenerated_msg_domain $(primary_domain)
    bounce {
        destination postmaster $(local_domains) {
            deliver_to &local_routing
        }
    }
}

# IMAP
imap tls://0.0.0.0:993 tcp://0.0.0.0:143 {
    auth &local_authdb
    storage &local_mailboxes
}

# Chatmail endpoint for user registration
chatmail tcp://0.0.0.0:80 {
    mail_domain $(primary_domain)
    mx_domain $(hostname)
    web_domain $(primary_domain)
    auth_db local_authdb
    storage local_mailboxes
}

chatmail tls://0.0.0.0:443 {
    mail_domain $(primary_domain)
    mx_domain $(hostname)
    web_domain $(primary_domain)
    auth_db local_authdb
    storage local_mailboxes
    tls file /etc/maddy/certs/yourdomain.com/fullchain.pem /etc/maddy/certs/yourdomain.com/privkey.pem
}
```

### 4. Set Up TLS Certificates

Using Let's Encrypt with certbot:

```bash
# Install certbot
sudo apt update && sudo apt install certbot

# Get certificates
sudo certbot certonly --standalone -d yourdomain.com -d mail.yourdomain.com

# Create symlinks for maddy
sudo ln -s /etc/letsencrypt/live/yourdomain.com/fullchain.pem /etc/maddy/certs/yourdomain.com/fullchain.pem
sudo ln -s /etc/letsencrypt/live/yourdomain.com/privkey.pem /etc/maddy/certs/yourdomain.com/privkey.pem

# Set proper permissions
sudo setfacl -R -m u:maddy:rX /etc/letsencrypt/{live,archive}
```

### 5. Configure DNS

Add these records to your DNS configuration:

```
; MX record - points email to your server
yourdomain.com.   MX    10 mail.yourdomain.com.

; A records - point domains to your server's IP
yourdomain.com.   A     YOUR_SERVER_IP
mail.yourdomain.com.   A     YOUR_SERVER_IP

; SPF - authorizes your server to send email for the domain
yourdomain.com.   TXT   "v=spf1 mx ~all"
mail.yourdomain.com. TXT   "v=spf1 a ~all"

; DMARC - policy for handling failed messages
_dmarc.yourdomain.com.   TXT   "v=DMARC1; p=quarantine; rua=mailto:postmaster@yourdomain.com"

; MTA-STS - requires TLS for email delivery
_mta-sts.yourdomain.com.   TXT   "v=STSv1; id=1"
_smtp._tls.yourdomain.com. TXT   "v=TLSRPTv1;rua=mailto:postmaster@yourdomain.com"
```

Generate DKIM keys and add the DNS record:

```bash
# Start maddy to generate DKIM keys
sudo -u maddy maddy --config /etc/maddy/maddy.conf run &
sleep 5
pkill maddy

# Get the DKIM record
sudo cat /var/lib/maddy/dkim_keys/yourdomain.com_default.dns

# Add this TXT record to DNS:
# default._domainkey.yourdomain.com. TXT "v=DKIM1; k=ed25519; p=YOUR_DKIM_KEY_HERE"
```

### 6. Set Up Systemd Service

Create `/etc/systemd/system/maddy.service`:

```systemd
[Unit]
Description=Maddy Mail Server
Documentation=man:maddy(1) man:maddy.conf(5) https://maddy.email
After=network-online.target
Wants=network-online.target

[Service]
Type=notify
NotifyAccess=main

User=maddy
Group=maddy

ConfigurationDirectory=maddy
RuntimeDirectory=maddy
StateDirectory=maddy
LogsDirectory=maddy
ReadOnlyPaths=/usr/lib/maddy
ReadWritePaths=/var/lib/maddy

PrivateTmp=true
PrivateHome=true
ProtectSystem=strict
ProtectKernelTunables=true
ProtectHostname=true
ProtectClock=true
ProtectControlGroups=true
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6
DeviceAllow=/dev/syslog

NoNewPrivileges=true
PrivateDevices=true
RestrictSUIDSGID=true
ProtectKernelModules=true
MemoryDenyWriteExecute=true
RestrictNamespaces=true
RestrictRealtime=true
LockPersonality=true

AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

UMask=0007
LimitNOFILE=131072
LimitNPROC=512

Restart=on-failure
RestartPreventExitStatus=2

ExecStart=/usr/local/bin/maddy --config /etc/maddy/maddy.conf run --libexec /usr/lib/maddy
ExecReload=/bin/kill -USR1 $MAINPID

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable maddy
sudo systemctl start maddy
sudo systemctl status maddy
```

## Testing Your Setup

### 1. Create a Test Account

```bash
# Create an admin account
sudo maddy --config /etc/maddy/maddy.conf creds create postmaster@yourdomain.com
sudo maddy --config /etc/maddy/maddy.conf imap-acct create postmaster@yourdomain.com
```

### 2. Test Email Delivery

Send a test email from another account to verify delivery.

### 3. Test Chatmail Registration

Visit `https://yourdomain.com` in your browser. You should see the chatmail registration interface with a QR code for Delta Chat.

## Chatmail Features

### Passwordless Registration

Users can create accounts instantly via the web interface without providing personal information. The system generates random usernames and passwords automatically.

### QR Code Integration

The web interface generates QR codes that users can scan with Delta Chat to instantly configure their accounts.

### Encryption-Only Messaging

The server is configured to only accept encrypted messages, ensuring privacy by default.

### Automatic Cleanup

Messages are automatically cleaned up after 20 days to minimize data retention.

## Monitoring and Maintenance

### Logs

View maddy logs:

```bash
sudo journalctl -u maddy -f
```

### Database Maintenance

The SQLite databases are self-maintaining, but you can monitor disk usage:

```bash
du -sh /var/lib/maddy/
```

### Certificate Renewal

Let's Encrypt certificates renew automatically, but you may need to reload maddy:

```bash
sudo systemctl reload maddy
```

## Troubleshooting

### Common Issues

1. **Port 25 blocked**: Some hosting providers block port 25. Use port 587/465 for submission instead.

2. **TLS certificate errors**: Ensure certificate paths are correct and maddy can read them.

3. **DNS propagation**: DNS changes can take up to 24 hours to propagate.

4. **Firewall issues**: Ensure ports 25, 80, 443, 587, 993 are open.

### Getting Help

- Check the [Maddy documentation](https://maddy.email)
- Join the IRC channel: `#maddy` on `irc.oftc.net`
- File issues on [GitHub](https://github.com/themadorg/madmail)

## Security Considerations

- Keep maddy updated with the latest security patches
- Use strong TLS certificates
- Monitor logs for suspicious activity
- Regularly backup your data
- Consider using fail2ban for additional protection

## Advanced Configuration

### Multiple Domains

To support multiple domains, add them to the `local_domains` variable:

```maddy
$(local_domains) = yourdomain.com otherdomain.com
```

### Custom Branding

Modify the web interface by editing the embedded HTML templates in the chatmail endpoint source code.

### Rate Limiting

Adjust rate limits in the configuration to prevent abuse:

```maddy
limits {
    all rate 50 1s  # Allow 50 messages per second
    all concurrency 20  # Allow 20 concurrent connections
}
```

### Using Caddy as Reverse Proxy

If you prefer to use Caddy as a reverse proxy in front of Maddy Chatmail, modify the Maddy configuration to use different ports for the chatmail endpoints (e.g., 8080 for HTTP and 8443 for HTTPS), and configure Caddy as follows:

```caddyfile
yourdomain.com {
    reverse_proxy localhost:8080
}

yourdomain.com:443 {
    tls /etc/caddy/certs/yourdomain.com/fullchain.pem /etc/caddy/certs/yourdomain.com/privkey.pem
    reverse_proxy localhost:8443
}
```

### Using Nginx as Reverse Proxy

Alternatively, you can use Nginx as a reverse proxy. First, change the chatmail endpoints in Maddy to use ports 8080 and 8443, then configure Nginx:

```nginx
server {
    listen 80;
    server_name yourdomain.com;

    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

server {
    listen 443 ssl;
    server_name yourdomain.com;

    ssl_certificate /etc/nginx/certs/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/nginx/certs/yourdomain.com/privkey.pem;

    location / {
        proxy_pass https://localhost:8443;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Contributing

This chatmail implementation is based on the Maddy Mail Server. Contributions to improve chatmail functionality are welcome on the [GitHub repository](https://github.com/themadorg/madmail).