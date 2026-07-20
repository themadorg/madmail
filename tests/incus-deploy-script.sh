#!/usr/bin/env bash
# Deploy a release madmail binary to a local Incus container.
# Persistent state is exposed on the host at ./data/incus-vm via an Incus storage volume.
#
# Modeled after tests/deltachat-test-deploy.py and tests/deltachat-test/utils/lxc.py.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DATA_DIR="$ROOT/data/incus-vm"
MOUNT_PID_FILE="$ROOT/data/.incus-vm-mount.pid"
CONTAINER="${MADMAIL_INCUS_NAME:-madmail-local}"
VOLUME="${CONTAINER}-data"
POOL="${MADMAIL_INCUS_POOL:-default}"
BINARY="${CHATMAIL_BIN:-$ROOT/target/release/madmail}"
CPU_LIMIT="${MADMAIL_INCUS_CPU:-1}"
MEM_LIMIT="${MADMAIL_INCUS_MEMORY:-1024MB}"
WITH_ADMIN=0
REBUILD=0
PURGE=0

usage() {
	cat <<EOF
Usage: $(basename "$0") [options]

Deploy madmail to local Incus with state in $DATA_DIR.

Options:
  --with-webadmin   Enable embedded admin UI at /admin
  --rebuild         Force static release rebuild before deploy
  --purge           Delete container, storage volume, and ./data/incus-vm (make incus-down)
  -h, --help        Show this help

Environment:
  MADMAIL_INCUS_NAME   Container name (default: madmail-local)
  MADMAIL_INCUS_POOL   Incus storage pool (default: default)
  CHATMAIL_BIN         Binary path (default: \$ROOT/target/release/madmail)
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--with-webadmin) WITH_ADMIN=1; shift ;;
		--rebuild) REBUILD=1; shift ;;
		--purge|--destroy) PURGE=1; shift ;;
		-h|--help) usage; exit 0 ;;
		*) echo "Unknown option: $1" >&2; usage >&2; exit 1 ;;
	esac
done

command -v incus >/dev/null || {
	echo "incus is required on PATH" >&2
	exit 1
}

container_exists() {
	incus list --format csv -c n 2>/dev/null | grep -Fxq "$CONTAINER"
}

volume_exists() {
	incus storage volume show "$POOL" "$VOLUME" &>/dev/null
}

stop_data_mount() {
	if [[ -f "$MOUNT_PID_FILE" ]]; then
		local pid
		pid="$(cat "$MOUNT_PID_FILE")"
		if kill -0 "$pid" 2>/dev/null; then
			kill "$pid" 2>/dev/null || true
		fi
		rm -f "$MOUNT_PID_FILE"
	fi
}

sync_data_dir() {
	local staging="${DATA_DIR}.staging.$$"
	rm -rf "$staging"
	mkdir -p "$staging"
	incus storage volume file pull -r "$POOL" "$VOLUME/" "$staging/" >/dev/null
	if [[ -d "$DATA_DIR" ]]; then
		rm -rf "${DATA_DIR}.bak.$$" 2>/dev/null || true
		mv "$DATA_DIR" "${DATA_DIR}.bak.$$"
		rm -rf "${DATA_DIR}.bak.$$" 2>/dev/null || true
	fi
	mv "$staging" "$DATA_DIR"
}

start_data_mount() {
	mkdir -p "$DATA_DIR"
	stop_data_mount

	if command -v sshfs >/dev/null; then
		incus storage volume file mount "$POOL" "$VOLUME" "$DATA_DIR" &
		echo $! >"$MOUNT_PID_FILE"
		for _ in $(seq 1 20); do
			if mountpoint -q "$DATA_DIR" 2>/dev/null; then
				return 0
			fi
			sleep 1
		done
		stop_data_mount
	fi

	echo "-- sshfs not available; syncing volume snapshot to $DATA_DIR ..."
	sync_data_dir
}

ensure_volume() {
	if volume_exists; then
		return
	fi
	echo "-- Creating Incus storage volume $VOLUME ..."
	incus storage volume create "$POOL" "$VOLUME"
}

ensure_data_device() {
	if incus config device get "$CONTAINER" madmail-data path &>/dev/null; then
		local source
		source="$(incus config device get "$CONTAINER" madmail-data source 2>/dev/null || true)"
		if [[ "$source" == "$VOLUME" ]]; then
			return
		fi
		echo "-- Replacing stale madmail-data device on $CONTAINER ..."
		incus config device remove "$CONTAINER" madmail-data
	fi

	echo "-- Attaching storage volume $VOLUME to /var/lib/madmail ..."
	incus config device add "$CONTAINER" madmail-data disk \
		pool="$POOL" source="$VOLUME" path=/var/lib/madmail
	incus restart "$CONTAINER"
	for _ in $(seq 1 30); do
		if incus exec "$CONTAINER" -- true 2>/dev/null; then
			break
		fi
		sleep 1
	done
}

if [[ $PURGE -eq 1 ]]; then
	stop_data_mount
	if container_exists; then
		echo "-- Deleting Incus container $CONTAINER ..."
		incus delete "$CONTAINER" --force
	else
		echo "-- Container $CONTAINER does not exist"
	fi
	if volume_exists; then
		echo "-- Deleting storage volume $VOLUME ..."
		incus storage volume delete "$POOL" "$VOLUME"
	fi
	if [[ -d "$DATA_DIR" ]]; then
		echo "-- Deleting host data $DATA_DIR ..."
		rm -rf "$DATA_DIR"
	fi
	rm -rf "${DATA_DIR}.staging."* "${DATA_DIR}.bak."* 2>/dev/null || true
	rm -f "$MOUNT_PID_FILE"
	echo "-- Incus madmail environment removed"
	exit 0
fi

if [[ $REBUILD -eq 1 ]] || [[ ! -f "$BINARY" ]]; then
	echo "-- Building static release binary ..."
	make -C "$ROOT" build-release-static
	BINARY="$ROOT/target/release/madmail"
fi

if [[ ! -x "$BINARY" ]]; then
	chmod +x "$BINARY"
fi

echo "-- Binary: $BINARY"
echo "-- Data:   $DATA_DIR (Incus volume $VOLUME)"
echo "-- Target: $CONTAINER"

ensure_resource_limits() {
	echo "-- Resource limits: ${CPU_LIMIT} CPU, ${MEM_LIMIT} memory"
	incus config set "$CONTAINER" limits.cpu="$CPU_LIMIT"
	incus config set "$CONTAINER" limits.memory="$MEM_LIMIT"
}

ensure_container() {
	if container_exists; then
		incus start "$CONTAINER" 2>/dev/null || true
		ensure_resource_limits
		return
	fi

	echo "-- Launching Debian 12 container $CONTAINER ..."
	incus launch images:debian/12 "$CONTAINER" \
		-c security.nesting=true \
		-c limits.cpu="$CPU_LIMIT" \
		-c limits.memory="$MEM_LIMIT"

	for _ in $(seq 1 30); do
		if incus exec "$CONTAINER" -- true 2>/dev/null; then
			break
		fi
		sleep 1
	done
}

wait_ipv4() {
	local ip=""
	for _ in $(seq 1 30); do
		ip="$(incus list "$CONTAINER" --format csv -c 4 2>/dev/null | head -1 | awk '{print $1}')"
		if [[ -n "$ip" && "$ip" != "-" ]]; then
			echo "$ip"
			return 0
		fi
		sleep 2
	done
	echo "Failed to obtain IPv4 for $CONTAINER" >&2
	exit 1
}

ensure_volume
ensure_container
ensure_data_device
start_data_mount

IP="$(wait_ipv4)"
echo "-- Container IP: $IP"

echo "-- Installing container dependencies (first run may take a minute) ..."
incus exec "$CONTAINER" -- bash -euo pipefail -c '
	if command -v madmail >/dev/null 2>&1; then
		exit 0
	fi
	export DEBIAN_FRONTEND=noninteractive
	apt-get -o DPkg::Lock::Timeout=120 update -qq
	apt-get install -y -qq ca-certificates curl iproute2 jq openssh-server
	apt-get clean
	rm -rf /var/lib/apt/lists/*
'

echo "-- Pushing madmail binary ..."
incus file push "$BINARY" "$CONTAINER/tmp/madmail" --mode=755 --uid=0 --gid=0

INSTALL_FLAGS=(
	"--simple"
	"--ip" "$IP"
	"--tls-mode" "self_signed"
	"--enable-chatmail"
	"--enable-iroh"
	"--non-interactive"
)

echo "-- Running madmail install ..."
incus exec "$CONTAINER" -- bash -euo pipefail -c "
	systemctl stop madmail 2>/dev/null || true
	/tmp/madmail install ${INSTALL_FLAGS[*]}
	sed -i 's/^log off\$/log syslog/' /etc/madmail/madmail.conf 2>/dev/null || true
	systemctl daemon-reload
	systemctl reset-failed madmail 2>/dev/null || true
	systemctl enable madmail
	systemctl restart madmail
"

if [[ $WITH_ADMIN -eq 1 ]]; then
	incus exec "$CONTAINER" -- bash -euo pipefail -c '
		madmail admin-web path /admin
		madmail admin-web enable
		systemctl restart madmail
	'
else
	incus exec "$CONTAINER" -- bash -euo pipefail -c 'madmail admin-web disable 2>/dev/null || true'
fi

incus exec "$CONTAINER" -- rm -f /tmp/madmail
sync_data_dir

echo ""
echo "== madmail deployed =="
echo "  container:  $CONTAINER"
echo "  ip:         $IP"
echo "  data:       $DATA_DIR"
echo "  volume:     $POOL/$VOLUME"
echo "  imap:       $IP:993 (TLS)"
echo "  smtp:       $IP:465 / $IP:587 (TLS)"
echo "  web:        https://$IP/"

TOKEN="$(incus exec "$CONTAINER" -- madmail admin-token --raw 2>/dev/null || true)"
if [[ -n "$TOKEN" ]]; then
	echo "  admin-token: $TOKEN"
fi

if [[ $WITH_ADMIN -eq 1 ]]; then
	echo "  admin-ui:   https://$IP/admin/"
fi

echo ""
echo "Logs:    incus exec $CONTAINER -- journalctl -u madmail -f"
echo "Shell:   incus exec $CONTAINER -- bash"
echo "Upgrade: CHATMAIL_BIN=$BINARY $0"
echo "Remove:  make incus-down"