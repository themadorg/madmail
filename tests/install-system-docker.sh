#!/usr/bin/env bash
# Isolated host-provisioning regression. Never run the inner script on the host.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${MADMAIL_INSTALL_TEST_BINARY:-$ROOT/target/debug/madmail}"
IMAGE="madmail-install-system:test"
[[ -x "$BINARY" ]] || { echo "Build madmail first: cargo build -p chatmail" >&2; exit 1; }
# The test image copies no sources: do not send the repository as build context.
docker build -t "$IMAGE" - < "$ROOT/tests/install-system.Dockerfile"
docker run --rm -i --network none -v "$BINARY:/input/madmail:ro" "$IMAGE" bash -s <<'INNER'
set -euo pipefail
trap 'echo "FAIL: installer E2E at line $LINENO" >&2' ERR
fail() { echo "FAIL: $*" >&2; exit 1; }
absent_user() { if id "$1" >/dev/null 2>&1; then fail "unexpected user $1"; fi; }
reject() {
    local pattern="$1"; shift
    if "$@" > /tmp/rejected.log 2>&1; then fail "command unexpectedly succeeded"; fi
    grep -q -- "$pattern" /tmp/rejected.log
}
json_check() {
    python3 - "$@" <<'PYJSON'
import json, sys
# Install includes human progress lines before its final JSON envelope.
lines = open(sys.argv[1]).read().splitlines()
envelopes = [json.loads(line) for line in lines if line.startswith('{')]
assert len(envelopes) == 1 and envelopes[0]['ok'], envelopes
for key, value in zip(sys.argv[2::2], sys.argv[3::2]):
    assert envelopes[0]['data'][key] == json.loads(value), (key, envelopes[0])
PYJSON
}
cp /input/madmail /tmp/madmail
chmod 755 /tmp/madmail
mkdir -p /var/xyzzy/{etc,lib,bin} /test-bin
# PID 1 is not systemd: record daemon-reload while testing real generated unit
# syntax with systemd-analyze below. No host systemd access is granted.
cat > /test-bin/systemctl <<'SYSTEMCTL'
#!/bin/sh
printf '%s\n' "$*" >> /tmp/systemctl-calls
exit "${MADMAIL_TEST_SYSTEMCTL_EXIT:-0}"
SYSTEMCTL
chmod 755 /test-bin/systemctl
export PATH="/test-bin:$PATH"
args=(install --simple --system --ip 127.0.0.1 --tls-mode self_signed --non-interactive
      --config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail
      --binary-path /var/xyzzy/bin/madmail)
reject 'cannot be used with' /tmp/madmail "${args[@]}" --local
for flag in --config-dir --state-dir --binary-path; do
    # Supply each option only once; clap rejects duplicated scalar options itself.
    reject 'must be absolute' /tmp/madmail install --simple --system --ip 127.0.0.1 \
        --tls-mode self_signed "$flag" relative/path --dry-run
done
reject 'control characters' /tmp/madmail install --simple --system --ip 127.0.0.1 \
    --tls-mode self_signed --config-dir $'/tmp/invalid\npath' --dry-run
/tmp/madmail --json "${args[@]}" --dry-run > /tmp/dry-json.log
json_check /tmp/dry-json.log dry_run true system_install true create_service_user true install_binary true install_systemd true
/tmp/madmail --json "${args[@]}" --dry-run --skip-user --skip-systemd > /tmp/skip-json.log
json_check /tmp/skip-json.log create_service_user false install_binary true install_systemd false
/tmp/madmail --json "${args[@]}" --dry-run --cert-only > /tmp/cert-json.log
json_check /tmp/cert-json.log install_binary false install_systemd false
/tmp/madmail --json install --simple --ip 127.0.0.1 --tls-mode self_signed --dry-run > /tmp/auto-system.log
json_check /tmp/auto-system.log system_install true
/tmp/madmail --json install --simple --local --ip 127.0.0.1 --tls-mode self_signed --dry-run > /tmp/forced-local.log
json_check /tmp/forced-local.log system_install false install_binary false install_systemd false
/tmp/madmail --json install --simple --ip 127.0.0.1 --tls-mode self_signed \
    --config-dir /tmp/auto-config --state-dir /tmp/auto-state --dry-run > /tmp/auto-local.log
json_check /tmp/auto-local.log system_install false
grep -q 'use --system' /tmp/auto-local.log
[[ ! -e /tmp/auto-config && ! -e /tmp/auto-state && ! -e /etc/madmail && ! -e /etc/systemd/system/madmail.service ]]
echo 'PASS: mode conflicts, automatic/explicit modes, JSON plans, invalid paths and certificate-only plan'
/tmp/madmail "${args[@]}" --dry-run > /tmp/dry-run.log
grep -q 'Install mode:  system' /tmp/dry-run.log
grep -q 'install and daemon-reload systemd unit' /tmp/dry-run.log
absent_user madmail
[[ ! -e /var/xyzzy/etc/madmail && ! -e /tmp/systemctl-calls ]]
if runuser -u nobody -- /tmp/madmail "${args[@]}" > /tmp/nonroot.log 2>&1; then
    echo 'Non-root system install unexpectedly succeeded'; exit 1
fi
grep -q 'system install requires root' /tmp/nonroot.log
/tmp/madmail install --simple --local --ip 127.0.0.1 --tls-mode self_signed \
    --config-dir /tmp/local-config --state-dir /tmp/local-state > /tmp/local.log
[[ -f /tmp/local-config/madmail.conf && ! -f /usr/share/man/man1/madmail.1 ]]
absent_user madmail
echo 'PASS: dry-run side effects, non-root system rejection and local host isolation'
/tmp/madmail --json "${args[@]}" > /tmp/install.log
json_check /tmp/install.log installed true system_install true service_installed true service_started false
[[ -f /usr/share/man/man1/madmail.1 && -f /usr/share/bash-completion/completions/madmail ]]
id madmail
[[ -x /var/xyzzy/bin/madmail ]]
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/credentials.db)" = madmail:madmail ]]
runuser -u madmail -- test -r /var/xyzzy/etc/madmail/certs/privkey.pem
grep -q '^ExecStart=/var/xyzzy/bin/madmail --config /var/xyzzy/etc/madmail/madmail.conf run --libexec /var/xyzzy/lib/madmail$' /etc/systemd/system/madmail.service
grep -q '^RuntimeDirectory=madmail$' /etc/systemd/system/madmail.service
if grep -q '^StateDirectory=' /etc/systemd/system/madmail.service; then fail 'custom unit uses default state directory'; fi
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
absent_user skippedmail
[[ -x /tmp/skipped-bin && ! -e /etc/systemd/system/skippedmail.service ]]
echo 'PASS: custom-path account, ownership, binary/docs, unit verification, service-user boot and idempotence'

# Skip-systemd alone must still create the account and apply ownership.
cp /tmp/madmail /tmp/nounitmail
reloads_before="$(wc -l < /tmp/systemctl-calls)"
/tmp/nounitmail --json install --simple --system --skip-systemd --ip 127.0.0.1 \
    --tls-mode self_signed --config-dir /tmp/nounit-config --state-dir /tmp/nounit-state \
    --binary-path /tmp/nounit-bin > /tmp/nounit.log
json_check /tmp/nounit.log system_install true service_installed false
id nounitmail >/dev/null
[[ "$(stat -c '%U:%G' /tmp/nounit-state/credentials.db)" = nounitmail:nounitmail ]]
[[ -x /tmp/nounit-bin && ! -e /etc/systemd/system/nounitmail.service ]]
[[ "$(wc -l < /tmp/systemctl-calls)" = "$reloads_before" ]]

# Skip-user alone must leave an existing account and file ownership alone but write the unit.
cp /tmp/madmail /tmp/nousermail
useradd -rU -d /original-home nousermail
/tmp/nousermail --json install --simple --system --skip-user --ip 127.0.0.1 \
    --tls-mode self_signed --config-dir /tmp/nouser-config --state-dir /tmp/nouser-state \
    --binary-path '/tmp/nouser bin%name' > /tmp/nouser.log
json_check /tmp/nouser.log system_install true service_installed true
[[ "$(getent passwd nousermail | cut -d: -f6)" = /original-home ]]
[[ "$(stat -c '%U:%G' /tmp/nouser-state/credentials.db)" = root:root ]]
[[ -x '/tmp/nouser bin%name' && -f /etc/systemd/system/nousermail.service ]]
systemd-analyze verify /etc/systemd/system/nousermail.service
[[ "$(wc -l < /tmp/systemctl-calls)" = "$((reloads_before + 1))" ]]
echo 'PASS: each skip flag independently and combined, JSON results, escaped binary path'

# Local bootstrap really works as a non-root user and does not copy a host binary.
install -d -o nobody -g nogroup /tmp/nonroot-local
runuser -u nobody -- /tmp/madmail --json install --simple --local --ip 127.0.0.1 \
    --tls-mode self_signed --config-dir /tmp/nonroot-local/config \
    --state-dir /tmp/nonroot-local/state --binary-path /tmp/should-not-copy > /tmp/nonroot-local.log
json_check /tmp/nonroot-local.log system_install false service_installed false
[[ -f /tmp/nonroot-local/config/madmail.conf && ! -e /tmp/should-not-copy ]]

# A failed systemctl invocation must not be reported as installation success.
reject 'systemctl daemon-reload failed' env MADMAIL_TEST_SYSTEMCTL_EXIT=1 /tmp/madmail "${args[@]}"
[[ -x /var/xyzzy/bin/madmail && -f /etc/systemd/system/madmail.service ]]
echo 'PASS: non-root local install and propagated systemd failure'
echo 'PASS: complete Unix installer E2E matrix'
INNER
