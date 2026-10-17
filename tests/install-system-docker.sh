#!/usr/bin/env bash
# Isolated host-provisioning regression. Never run the inner script on the host.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${MADMAIL_INSTALL_TEST_BINARY:-$ROOT/target/debug/madmail}"
IMAGE="madmail-install-system:test"
[[ -x "$BINARY" ]] || { echo "Build madmail first: cargo build -p chatmail" >&2; exit 1; }
docker build -f "$ROOT/tests/install-system.Dockerfile" -t "$IMAGE" "$ROOT"
docker run --rm -i --network none -v "$BINARY:/input/madmail:ro" "$IMAGE" bash -s <<'INNER'
set -euo pipefail
cp /input/madmail /tmp/madmail
chmod 755 /tmp/madmail
mkdir -p /var/xyzzy/{etc,lib,bin} /test-bin
# PID 1 is not systemd: record daemon-reload while testing real generated unit
# syntax with systemd-analyze below. No host systemd access is granted.
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> /tmp/systemctl-calls\n' > /test-bin/systemctl
chmod 755 /test-bin/systemctl
export PATH="/test-bin:$PATH"
args=(install --simple --system --ip 127.0.0.1 --tls-mode self_signed --non-interactive
      --config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail
      --binary-path /var/xyzzy/bin/madmail)
/tmp/madmail "${args[@]}" --dry-run > /tmp/dry-run.log
grep -q 'Install mode:  system' /tmp/dry-run.log
grep -q 'install and daemon-reload systemd unit' /tmp/dry-run.log
! id madmail >/dev/null 2>&1
[[ ! -e /var/xyzzy/etc/madmail && ! -e /tmp/systemctl-calls ]]
if runuser -u nobody -- /tmp/madmail "${args[@]}" > /tmp/nonroot.log 2>&1; then
    echo 'Non-root system install unexpectedly succeeded'; exit 1
fi
grep -q 'system install requires root' /tmp/nonroot.log
/tmp/madmail install --simple --local --ip 127.0.0.1 --tls-mode self_signed \
    --config-dir /tmp/local-config --state-dir /tmp/local-state > /tmp/local.log
[[ -f /tmp/local-config/madmail.conf && ! -f /usr/share/man/man1/madmail.1 ]]
! id madmail >/dev/null 2>&1
/tmp/madmail "${args[@]}" > /tmp/install.log
id madmail
[[ -x /var/xyzzy/bin/madmail ]]
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/credentials.db)" = madmail:madmail ]]
runuser -u madmail -- test -r /var/xyzzy/etc/madmail/certs/privkey.pem
grep -q '^ExecStart=/var/xyzzy/bin/madmail --config /var/xyzzy/etc/madmail/madmail.conf run --libexec /var/xyzzy/lib/madmail$' /etc/systemd/system/madmail.service
grep -q '^RuntimeDirectory=madmail$' /etc/systemd/system/madmail.service
! grep -q '^StateDirectory=' /etc/systemd/system/madmail.service
grep -q '^daemon-reload$' /tmp/systemctl-calls
systemd-analyze verify /etc/systemd/system/madmail.service
# RuntimeDirectory is normally created by systemd when starting the service.
install -d -o madmail -g madmail /run/madmail
runuser -u madmail -- /var/xyzzy/bin/madmail --config /var/xyzzy/etc/madmail/madmail.conf \
    --state-dir /var/xyzzy/lib/madmail --boot-once > /tmp/boot.log 2>&1
uid_before="$(id -u madmail)"
/tmp/madmail "${args[@]}" > /tmp/reinstall.log
[[ "$(id -u madmail)" = "$uid_before" ]]
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
# Independent skip flags still skip system steps while copying the requested binary.
cp /tmp/madmail /tmp/skippedmail
/tmp/skippedmail install --simple --system --skip-user --skip-systemd \
    --ip 127.0.0.1 --tls-mode self_signed --config-dir /tmp/skipped-config \
    --state-dir /tmp/skipped-state --binary-path /tmp/skipped-bin > /tmp/skipped.log
! id skippedmail >/dev/null 2>&1
[[ -x /tmp/skipped-bin && ! -e /etc/systemd/system/skippedmail.service ]]
echo 'PASS: custom-path system install, dry-run, non-root rejection, local isolation, ownership, binary, unit verification, service-user boot, idempotence and skip flags'
INNER
