#!/usr/bin/env bash
# Isolated host-provisioning regression. Never run the inner script on the host.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BINARY="${MADMAIL_INSTALL_TEST_BINARY:-$ROOT/target/debug/madmail}"
IMAGE="madmail-install-system:test"
[[ -x "$BINARY" ]] || { echo "Build madmail first: cargo build -p chatmail" >&2; exit 1; }
# The container runs the binary: a macOS (Mach-O) host build fails with an exec error.
[[ "$(head -c4 "$BINARY" | od -An -tx1 | tr -d ' \n')" == 7f454c46 ]] || {
    echo "$BINARY is not a Linux ELF binary; set MADMAIL_INSTALL_TEST_BINARY to a Linux build" >&2
    exit 1
}
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
mkdir -p /test-bin
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
[[ ! -e /var/xyzzy && ! -e /tmp/systemctl-calls ]]
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
# A binary path inside the state directory is refused before anything changes.
reject 'inside the state directory' /tmp/madmail install --simple --system --ip 127.0.0.1 \
    --tls-mode self_signed --non-interactive --config-dir /var/xyzzy/etc/madmail \
    --state-dir /var/xyzzy/lib/madmail --binary-path /var/xyzzy/lib/madmail/bin/madmail
[[ ! -e /var/xyzzy/lib/madmail/bin ]]
# Existing users can migrate into a tree whose parents do not exist.
echo preserved > /var/xyzzy/lib/madmail/migration-marker
/tmp/madmail install --simple --system --ip 127.0.0.1 --tls-mode self_signed \
    --config-dir /home/mmcfg/madmail --state-dir /home/mmstate/madmail \
    --binary-path /var/xyzzy/bin/madmail > /tmp/migrate.log
[[ "$(getent passwd madmail | cut -d: -f6)" = /home/mmstate/madmail ]]
[[ -f /home/mmstate/madmail/migration-marker ]]
grep -q '^ProtectHome=false$' /etc/systemd/system/madmail.service
systemd-analyze verify /etc/systemd/system/madmail.service
runuser -u madmail -- test -r /home/mmstate/madmail/credentials.db

# Simulate usermod updating passwd before a failed move; rollback must use no -m.
cat > /test-bin/usermod <<'USERMOD'
#!/bin/bash
if [[ " $* " == *" -m "* ]]; then
    /usr/sbin/usermod -d "$2" "${@: -1}"
    exit 1
fi
exec /usr/sbin/usermod "$@"
USERMOD
chmod 755 /test-bin/usermod
reject 'previous home restored' /tmp/madmail "${args[@]}"
[[ "$(getent passwd madmail | cut -d: -f6)" = /home/mmstate/madmail ]]
[[ -f /home/mmstate/madmail/migration-marker ]]
rm /test-bin/usermod
/tmp/madmail "${args[@]}" > /tmp/migrate-back.log
[[ -f /var/xyzzy/lib/madmail/migration-marker ]]
grep -q '^ProtectHome=true$' /etc/systemd/system/madmail.service
# A full install into a pre-created state dir (e.g. a mount point) must not
# repoint the account and leave the data in its previous home behind.
mkdir -p /srv/premade/madmail
reject 'would leave the data in /var/xyzzy/lib/madmail behind' /tmp/madmail install --simple \
    --system --ip 127.0.0.1 --tls-mode self_signed --config-dir /var/xyzzy/etc/madmail \
    --state-dir /srv/premade/madmail --binary-path /var/xyzzy/bin/madmail
[[ "$(getent passwd madmail | cut -d: -f6)" = /var/xyzzy/lib/madmail ]]
[[ -f /var/xyzzy/lib/madmail/migration-marker && -z "$(ls -A /srv/premade/madmail)" ]]
grep -q '^ProtectHome=true$' /etc/systemd/system/madmail.service
rmdir /srv/premade/madmail
echo 'PASS: missing parents, home paths, migration data preservation, failed-move rollback and pre-created state refusal'

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
# WorkingDirectory= is taken verbatim; ReadWritePaths= and ExecStart= are quoted.
cp /tmp/madmail /tmp/spacemail
useradd -rU -d /nonexistent spacemail
/tmp/spacemail install --simple --system --skip-user --ip 127.0.0.1 \
    --tls-mode self_signed --config-dir '/tmp/space config' --state-dir '/tmp/space state' \
    --binary-path /tmp/space-bin > /tmp/space.log
grep -qx 'WorkingDirectory=/tmp/space state' /etc/systemd/system/spacemail.service
grep -qx 'ReadWritePaths="/tmp/space state" "/tmp/space config"' /etc/systemd/system/spacemail.service
systemd-analyze verify /etc/systemd/system/spacemail.service
echo 'PASS: each skip flag independently and combined, JSON results, escaped binary and state paths'

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
# Isolated repair: no DNS/network, TLS, configuration, DB or binary writes.
repair_paths=(--config-dir /var/xyzzy/etc/madmail --state-dir /var/xyzzy/lib/madmail
              --binary-path /var/xyzzy/bin/madmail)
for mode in --user-only --systemd-only; do
    for conflict in --local --cert-only --skip-user --skip-systemd --install-service --start-service --firewall; do
        reject 'cannot be used with' /tmp/madmail install "$mode" "$conflict"
    done
    for flag in --config-dir --state-dir --binary-path; do
        reject 'must be absolute' /tmp/madmail install "$mode" "$flag" relative --dry-run
    done
    reject 'control characters' /tmp/madmail install "$mode" --state-dir $'/tmp/bad\npath' --dry-run
    reject 'requires root' runuser -u nobody -- /tmp/madmail install "$mode" "${repair_paths[@]}"
    reject 'existing config file' /tmp/madmail install "$mode" --config-dir /tmp/missing-repair
    runuser -u nobody -- /tmp/madmail --json install "$mode" --dry-run \
        --config-dir /tmp/dry-repair --state-dir /tmp/dry-state > /tmp/repair-dry.log
    json_check /tmp/repair-dry.log dry_run true install_binary false service_installed false
    [[ ! -e /tmp/dry-repair && ! -e /tmp/dry-state ]]
done
reject 'cannot be used with' /tmp/madmail install --user-only --systemd-only

# Snapshot every persisted file and binary, including arbitrary operator config edits.
printf '\n# operator customization must survive repair\n' >> /var/xyzzy/etc/madmail/madmail.conf
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -type f -exec sha256sum {} + | sort > /tmp/repair-files.before
sha256sum /etc/systemd/system/madmail.service > /tmp/repair-unit.before
reloads_before="$(wc -l < /tmp/systemctl-calls)"
userdel madmail
# Repair must also handle a retained group from a partially deleted account.
getent group madmail >/dev/null || groupadd -r madmail
retained_gid="$(getent group madmail | cut -d: -f3)"
/tmp/madmail --json install --user-only "${repair_paths[@]}" > /tmp/user-repair.log
json_check /tmp/user-repair.log create_service_user true apply_permissions true install_binary false install_systemd false
id madmail >/dev/null
[[ "$(id -g madmail)" = "$retained_gid" ]]
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/credentials.db)" = madmail:madmail ]]
sha256sum -c /tmp/repair-unit.before >/dev/null
[[ "$(wc -l < /tmp/systemctl-calls)" = "$reloads_before" ]]
repair_uid="$(id -u madmail)"
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-repair-repeat.log
[[ "$(id -u madmail)" = "$repair_uid" ]]
# An existing account with another home is repointed at the existing state
# directory without moving it, and ownership is reapplied after the account fix.
usermod -d /var/wrong/madmail madmail
chown root:root /var/xyzzy/etc/madmail/madmail.conf
chmod 600 /var/xyzzy/etc/madmail/madmail.conf
# Links planted in the service-writable state tree must never be chowned through.
ln -s /etc/shadow /var/xyzzy/lib/madmail/planted-file
ln -s /etc /var/xyzzy/lib/madmail/planted-dir
shadow_owner="$(stat -c '%U:%G' /etc/shadow)"
# A TLS path through a linked directory in the state tree is not chowned either.
mkdir /tmp/root-tls
printf 'x' > /tmp/root-tls/fullchain.pem
ln -s /tmp/root-tls /var/xyzzy/lib/madmail/tls
/tmp/madmail install --user-only "${repair_paths[@]}" \
    --cert-path /var/xyzzy/lib/madmail/tls/fullchain.pem > /tmp/user-planted-tls.log
grep -q 'Skipping ownership of /var/xyzzy/lib/madmail/tls/fullchain.pem' /tmp/user-planted-tls.log
[[ "$(stat -c '%U:%G' /tmp/root-tls/fullchain.pem)" = root:root ]]
rm /var/xyzzy/lib/madmail/tls
# Nested planted links are skipped by the descriptor walk as well.
mkdir -p /var/xyzzy/lib/madmail/nested/deeper
ln -s /etc/passwd /var/xyzzy/lib/madmail/nested/deeper/planted-passwd
ln -s /etc /var/xyzzy/lib/madmail/nested/planted-etc
# A TLS path outside the state tree that resolves into it (through an operator
# link) is not chowned by path: the service's own link there would redirect it.
mkdir /var/xyzzy/lib/madmail/op-tls
ln -s /etc/shadow /var/xyzzy/lib/madmail/op-tls/fullchain.pem
ln -s /var/xyzzy/lib/madmail/op-tls /tmp/op-tls
/tmp/madmail install --user-only "${repair_paths[@]}" \
    --cert-path /tmp/op-tls/fullchain.pem > /tmp/user-op-tls.log
grep -q 'Skipping ownership of /tmp/op-tls/fullchain.pem' /tmp/user-op-tls.log
[[ "$(stat -c '%U:%G' /etc/shadow)" = "$shadow_owner" ]]
# The same for a file-level operator link to a TLS file in the tree that the
# service account replaced with a link out of it: resolving the whole path ends
# at /etc/shadow, but the operator link's first hop lands in the tree.
mkdir /var/xyzzy/lib/madmail/op-file
ln -s /etc/shadow /var/xyzzy/lib/madmail/op-file/fullchain.pem
ln -s /var/xyzzy/lib/madmail/op-file/fullchain.pem /tmp/op-file.pem
/tmp/madmail install --user-only "${repair_paths[@]}" \
    --cert-path /tmp/op-file.pem > /tmp/user-op-file.log
grep -q 'Skipping ownership of /tmp/op-file.pem' /tmp/user-op-file.log
[[ "$(stat -c '%U:%G' /etc/shadow)" = "$shadow_owner" ]]
rm -r /var/xyzzy/lib/madmail/op-file /tmp/op-file.pem
[[ "$(stat -c '%U:%G' /etc)" = root:root && "$(stat -c '%U' /etc/passwd)" = root ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/nested/deeper)" = madmail:madmail ]]
[[ "$(stat -c '%U' /var/xyzzy/lib/madmail/nested/deeper/planted-passwd)" = root ]]
rm -r /var/xyzzy/lib/madmail/nested /var/xyzzy/lib/madmail/op-tls /tmp/op-tls
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-repoint.log
[[ "$(getent passwd madmail | cut -d: -f6)" = /var/xyzzy/lib/madmail ]]
[[ "$(id -u madmail)" = "$repair_uid" ]]
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/credentials.db)" = madmail:madmail ]]
[[ "$(stat -c '%U:%G' /etc/shadow)" = "$shadow_owner" ]]
[[ "$(stat -c '%U:%G' /etc)" = root:root && "$(stat -c '%U' /etc/passwd)" = root ]]
rm /var/xyzzy/lib/madmail/planted-file /var/xyzzy/lib/madmail/planted-dir

# Unit-only preflights fail before replacing the unit or reloading systemd.
chmod 644 /var/xyzzy/bin/madmail
reject 'executable binary' /tmp/madmail install --systemd-only "${repair_paths[@]}"
chmod 755 /var/xyzzy/bin/madmail
userdel madmail
reject 'not found' /tmp/madmail install --systemd-only "${repair_paths[@]}"
sha256sum -c /tmp/repair-unit.before >/dev/null
[[ "$(wc -l < /tmp/systemctl-calls)" = "$reloads_before" ]]
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/restore-user.log
sha256sum /etc/passwd /etc/group > /tmp/repair-accounts.before
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -printf '%p %u %g %m\n' | sort > /tmp/repair-modes.before
rm /etc/systemd/system/madmail.service
/tmp/madmail --json install --systemd-only "${repair_paths[@]}" > /tmp/unit-repair.log
json_check /tmp/unit-repair.log create_service_user false apply_permissions false install_binary false service_installed true service_started false
systemd-analyze verify /etc/systemd/system/madmail.service
sha256sum -c /tmp/repair-unit.before >/dev/null
/tmp/madmail install --systemd-only "${repair_paths[@]}" > /tmp/unit-repair-repeat.log
sha256sum -c /tmp/repair-accounts.before >/dev/null
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -printf '%p %u %g %m\n' | sort > /tmp/repair-modes.after
diff -u /tmp/repair-modes.before /tmp/repair-modes.after
[[ "$(wc -l < /tmp/systemctl-calls)" = "$((reloads_before + 2))" ]]
if grep -v '^daemon-reload$' /tmp/systemctl-calls; then fail 'unexpected systemctl operation'; fi
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -type f -exec sha256sum {} + | sort > /tmp/repair-files.after
diff -u /tmp/repair-files.before /tmp/repair-files.after
# A masked unit or a unit symlinked elsewhere is refused, never written through.
reloads_before="$(wc -l < /tmp/systemctl-calls)"
mv /etc/systemd/system/madmail.service /tmp/madmail.service.saved
ln -s /dev/null /etc/systemd/system/madmail.service
reject 'is a symlink' /tmp/madmail install --systemd-only "${repair_paths[@]}"
[[ "$(readlink /etc/systemd/system/madmail.service)" = /dev/null ]]
# A full install checks the unit path before changing the account or any file.
sha256sum /etc/passwd /etc/group > /tmp/masked-accounts.before
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -printf '%p %u %g %m\n' | sort > /tmp/masked-modes.before
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -type f -exec sha256sum {} + | sort > /tmp/masked-files.before
reject 'is a symlink' /tmp/madmail "${args[@]}"
sha256sum -c /tmp/masked-accounts.before >/dev/null
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -printf '%p %u %g %m\n' | sort | diff -u /tmp/masked-modes.before -
find /var/xyzzy/etc/madmail /var/xyzzy/lib/madmail /var/xyzzy/bin -type f -exec sha256sum {} + | sort | diff -u /tmp/masked-files.before -
[[ "$(readlink /etc/systemd/system/madmail.service)" = /dev/null ]]
printf '[Unit]\nDescription=other\n' > /tmp/other.service
sha256sum /tmp/other.service > /tmp/other-unit.before
ln -sfn /tmp/other.service /etc/systemd/system/madmail.service
reject 'systemctl unmask madmail' /tmp/madmail install --systemd-only "${repair_paths[@]}"
sha256sum -c /tmp/other-unit.before >/dev/null
[[ -L /etc/systemd/system/madmail.service ]]
[[ "$(wc -l < /tmp/systemctl-calls)" = "$reloads_before" ]]
rm /etc/systemd/system/madmail.service
# The unit mode does not depend on the caller's umask.
(umask 077; /tmp/madmail install --systemd-only "${repair_paths[@]}" > /tmp/unit-unmasked.log)
[[ -f /etc/systemd/system/madmail.service && ! -L /etc/systemd/system/madmail.service ]]
[[ "$(stat -c '%a' /etc/systemd/system/madmail.service)" = 644 ]]
[[ ! -e /etc/systemd/system/madmail.service.tmp ]]
sha256sum -c /tmp/repair-unit.before >/dev/null
reject 'systemctl daemon-reload failed' env MADMAIL_TEST_SYSTEMCTL_EXIT=1 /tmp/madmail install --systemd-only "${repair_paths[@]}"
echo 'PASS: isolated repair conflicts, prerequisites, dry-run, account/unit restoration, home repointing, planted-link and masked-unit refusal, unit mode, content preservation, repeatability and failure propagation'

# The state walk changes a hard-linked file only when it belongs to the service
# account. A root-owned file outside the tree, linked into it (here by root; the
# service can do the same itself when fs.protected_hardlinks is 0), keeps its
# owner and mode.
printf 'victim\n' > /var/xyzzy/hl-victim
chmod 600 /var/xyzzy/hl-victim
ln /var/xyzzy/hl-victim /var/xyzzy/lib/madmail/hl-planted
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-hardlink.log
grep -q '^   ! Ownership of /var/xyzzy/lib/madmail/hl-planted left unchanged: belongs to uid 0, not to the service account and has 2 hard links' /tmp/user-hardlink.log
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/hl-victim)" = root:root:600 ]]
rm /var/xyzzy/lib/madmail/hl-planted /var/xyzzy/hl-victim
# The link count proves nothing: once /etc/shadow is replaced by a rename (as
# passwd, useradd or a package update do), a hard link planted in the tree is
# the old inode's only link. It still belongs to root and is left alone.
ln /etc/shadow /var/xyzzy/lib/madmail/x
cp -p /etc/shadow /etc/shadow.new
mv /etc/shadow.new /etc/shadow
[[ "$(stat -c '%U:%h' /var/xyzzy/lib/madmail/x)" = root:1 ]]
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-renamed-shadow.log
grep -q '^   ! Ownership of /var/xyzzy/lib/madmail/x left unchanged: belongs to uid 0, not to the service account;' /tmp/user-renamed-shadow.log
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/x)" = "$(stat -c '%U:%G' /etc/shadow)" ]]
rm /var/xyzzy/lib/madmail/x
# A planted link to a root-owned file outside the tree changes neither its
# owner nor its content.
shadow_sum="$(sha256sum /etc/shadow)"
shadow_meta="$(stat -c '%U:%G:%a' /etc/shadow)"
ln -s /etc/shadow /var/xyzzy/lib/madmail/planted-shadow
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-symlink.log
[[ "$(sha256sum /etc/shadow)" = "$shadow_sum" && "$(stat -c '%U:%G:%a' /etc/shadow)" = "$shadow_meta" ]]
rm /var/xyzzy/lib/madmail/planted-shadow
# When the account is recreated with a new uid and the old uid is free, the
# service's own hard-linked files still follow: they belong to the previous
# owner of the state directory.
old_uid="$(id -u madmail)"
install -o madmail -g madmail -m 640 /dev/null /var/xyzzy/lib/madmail/own-a
ln /var/xyzzy/lib/madmail/own-a /var/xyzzy/lib/madmail/own-b
userdel madmail
getent group madmail >/dev/null || groupadd -r madmail
new_uid=$((old_uid + 100))
while getent passwd "$new_uid" >/dev/null; do new_uid=$((new_uid + 1)); done
useradd -r -M -g madmail -u "$new_uid" -d /var/xyzzy/lib/madmail -s /sbin/nologin madmail
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-newuid.log
[[ "$(id -u madmail)" = "$new_uid" ]]
[[ "$(stat -c '%u:%h' /var/xyzzy/lib/madmail/own-a)" = "$new_uid:2" ]]
[[ "$(stat -c '%u' /var/xyzzy/lib/madmail)" = "$new_uid" ]]
[[ "$(stat -c '%U:%G' /var/xyzzy/lib/madmail/credentials.db)" = madmail:madmail ]]
# When the old uid now belongs to another account, files of that uid are that
# account's as far as install can tell: they are listed and left alone.
squat_uid="$(id -u madmail)"
userdel madmail
getent group madmail >/dev/null || groupadd -r madmail
useradd -r -M -N -u "$squat_uid" -d /nonexistent uidsquatter
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/user-squatted.log
[[ "$(id -u madmail)" != "$squat_uid" ]]
grep -q "belonged to uid $squat_uid, which is now account uidsquatter" /tmp/user-squatted.log
grep -q "^   ! Ownership of /var/xyzzy/lib/madmail/.* left unchanged: belongs to uid $squat_uid, not to the service account" /tmp/user-squatted.log
[[ "$(stat -c '%u' /var/xyzzy/lib/madmail/own-a)" = "$squat_uid" ]]
[[ "$(stat -c '%U' /var/xyzzy/lib/madmail)" = madmail ]]
rm /var/xyzzy/lib/madmail/own-a /var/xyzzy/lib/madmail/own-b
# The operator hands the rest over after checking it.
find /var/xyzzy/lib/madmail -uid "$squat_uid" -exec chown -h madmail:madmail {} +
userdel uidsquatter
# An operator link from the config directory to a TLS file in the state tree,
# whose target the service account then replaced with a link to /etc/shadow, is
# never written, chowned or chmodded through, by a full reinstall or by repair.
mv /var/xyzzy/etc/madmail/certs/fullchain.pem /tmp/fullchain.saved
install -d -o madmail -g madmail /var/xyzzy/lib/madmail/certs
cp /tmp/fullchain.saved /var/xyzzy/lib/madmail/certs/fullchain.pem
ln -s /var/xyzzy/lib/madmail/certs/fullchain.pem /var/xyzzy/etc/madmail/certs/fullchain.pem
runuser -u madmail -- ln -sfn /etc/shadow /var/xyzzy/lib/madmail/certs/fullchain.pem
/tmp/madmail "${args[@]}" > /tmp/reinstall-op-link.log
[[ "$(sha256sum /etc/shadow)" = "$shadow_sum" && "$(stat -c '%U:%G:%a' /etc/shadow)" = "$shadow_meta" ]]
runuser -u madmail -- ln -sfn /etc/shadow /var/xyzzy/lib/madmail/certs/fullchain.pem
/tmp/madmail install --user-only "${repair_paths[@]}" > /tmp/repair-op-link.log
grep -q 'Skipping ownership of /var/xyzzy/etc/madmail/certs/fullchain.pem: inside the service-writable state directory' /tmp/repair-op-link.log
[[ "$(sha256sum /etc/shadow)" = "$shadow_sum" && "$(stat -c '%U:%G:%a' /etc/shadow)" = "$shadow_meta" ]]
rm /var/xyzzy/etc/madmail/certs/fullchain.pem
rm -r /var/xyzzy/lib/madmail/certs
mv /tmp/fullchain.saved /var/xyzzy/etc/madmail/certs/fullchain.pem
# A --skip-user reinstall writes into the service's tree through descriptors
# too: a DKIM TXT file the service replaced with a link to /etc/shadow is
# replaced, not written through. DKIM keys need a DNS name, not an IP.
dns_args=(install --simple --system --domain mail.example.org --ip 127.0.0.1
          --tls-mode self_signed --non-interactive --config-dir /var/xyzzy/etc/madmail
          --state-dir /var/xyzzy/lib/madmail --binary-path /var/xyzzy/bin/madmail)
/tmp/madmail "${dns_args[@]}" > /tmp/reinstall-dns.log
[[ -f /var/xyzzy/lib/madmail/dkim/default.txt ]]
runuser -u madmail -- ln -sfn /etc/shadow /var/xyzzy/lib/madmail/dkim/default.txt
/tmp/madmail "${dns_args[@]}" --skip-user > /tmp/skip-user-reinstall.log
[[ "$(sha256sum /etc/shadow)" = "$shadow_sum" && "$(stat -c '%U:%G:%a' /etc/shadow)" = "$shadow_meta" ]]
[[ -f /var/xyzzy/lib/madmail/dkim/default.txt && ! -L /var/xyzzy/lib/madmail/dkim/default.txt ]]
grep -q '^v=DKIM1' /var/xyzzy/lib/madmail/dkim/default.txt
[[ "$(stat -c '%U:%G:%a' /var/xyzzy/etc/madmail/madmail.conf)" = root:madmail:640 ]]
/tmp/madmail "${dns_args[@]}" > /tmp/reinstall-after-skip.log
[[ "$(stat -c '%U' /var/xyzzy/lib/madmail/dkim/default.txt)" = madmail ]]
/tmp/madmail "${args[@]}" > /tmp/reinstall-ip.log
# Without the account, --skip-user refuses a state directory another account owns.
install -d -o nobody /tmp/foreign-state
cp /tmp/madmail /tmp/ghostmail
reject 'does not exist, but state directory' /tmp/ghostmail install --simple --system --skip-user \
    --skip-systemd --ip 127.0.0.1 --tls-mode self_signed --config-dir /tmp/ghost-config \
    --state-dir /tmp/foreign-state --binary-path /tmp/ghost-bin
[[ ! -e /tmp/ghost-config && ! -e /tmp/ghost-bin ]]
echo 'PASS: state walk skips foreign files (hard-linked or renamed), planted links and files of a reused uid, follows a new service uid, never writes through an operator link into the tree, and --skip-user writes through descriptors'

# Uninstall after an install with a shared --config-dir removes only the files
# install put there and leaves the directory and everything else in it.
mkdir -p /usr/local/etc
echo keep > /usr/local/etc/other.conf
/tmp/madmail install --simple --system --ip 127.0.0.1 --tls-mode self_signed \
    --non-interactive --config-dir /usr/local/etc --state-dir /var/xyzzy/lib/madmail \
    --binary-path /var/xyzzy/bin/madmail > /tmp/shared-install.log
grep -q -- '--config /usr/local/etc/madmail.conf' /etc/systemd/system/madmail.service
[[ -f /usr/local/etc/madmail.conf && -f /usr/local/etc/certs/privkey.pem ]]
/tmp/madmail uninstall --dry-run > /tmp/uninstall-plan.log
grep -qx 'Config file (removed): /usr/local/etc/madmail.conf' /tmp/uninstall-plan.log
grep -qx 'Left in place: /usr/local/etc (well-known system directory; only madmail.conf is removed)' /tmp/uninstall-plan.log
grep -q '^Left in place: /usr/local/etc/certs/privkey.pem (TLS file' /tmp/uninstall-plan.log
grep -qx 'State dir (removed with all contents): /var/xyzzy/lib/madmail' /tmp/uninstall-plan.log
if grep -q '^Config dir.*/usr/local/etc$' /tmp/uninstall-plan.log; then fail 'shared config dir planned for removal'; fi
# A shared state directory is kept even when named with --state-dir.
mkdir -p /srv/shared-data
/tmp/madmail uninstall --dry-run --state-dir /srv/shared-data > /tmp/uninstall-shared-state.log
grep -qx 'Left in place: /srv/shared-data (shared directory, not named for madmail; remove it yourself)' /tmp/uninstall-shared-state.log
if grep -q '^State dir.*/srv/shared-data$' /tmp/uninstall-shared-state.log; then fail 'shared state dir planned for removal'; fi
/tmp/madmail --json uninstall --dry-run > /tmp/uninstall-plan.json
python3 - /tmp/uninstall-plan.json <<'PYJSON'
import json, sys
envelope = [json.loads(l) for l in open(sys.argv[1]) if l.startswith('{')][-1]
left = {entry['path'] for entry in envelope['data']['left_in_place']}
assert {'/usr/local/etc', '/usr/local/etc/certs/fullchain.pem', '/usr/local/etc/certs/privkey.pem'} <= left, left
PYJSON
[[ -f /usr/local/etc/madmail.conf && -d /var/xyzzy/lib/madmail ]]
# A parent writable by a system group the service is not in (Ubuntu's /var/log
# is root:syslog 0775) does not block removing madmail's directory in it.
groupadd -r syslogish
chgrp syslogish /var/log
chmod 775 /var/log
mkdir -p /var/log/madmail
/tmp/madmail uninstall --force > /tmp/uninstall.log
[[ ! -e /var/log/madmail ]]
chgrp root /var/log
chmod 755 /var/log
[[ "$(cat /usr/local/etc/other.conf)" = keep ]]
[[ ! -e /usr/local/etc/madmail.conf && -f /usr/local/etc/certs/privkey.pem ]]
grep -q '^Left in place: /usr/local/etc/certs/fullchain.pem (TLS file' /tmp/uninstall.log
[[ ! -e /var/xyzzy/lib/madmail && ! -e /etc/systemd/system/madmail.service && ! -e /var/xyzzy/bin/madmail ]]
absent_user madmail
[[ -d /srv/shared-data ]]
echo 'PASS: uninstall keeps a shared config directory and its other files and a shared state directory, removes the dedicated state directory'
echo 'PASS: complete Unix installer E2E matrix'
INNER
