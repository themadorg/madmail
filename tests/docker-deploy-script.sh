#!/usr/bin/env bash
# Build and run madmail in Docker for local development.
# State lives under ./data/docker-vm (lib, etc, run bind mounts).
#
# Modeled after scripts/docker-turn-e2e.sh and docs/guide/docker.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DATA_DIR="$ROOT/data/docker-vm"
LIB_DIR="$DATA_DIR/lib"
ETC_DIR="$DATA_DIR/etc"
RUN_DIR="$DATA_DIR/run"
IMAGE="${MADMAIL_DOCKER_IMAGE:-madmail-local:docker}"
CONTAINER="${MADMAIL_DOCKER_NAME:-madmail-docker}"
HOST_IP="${MADMAIL_DOCKER_HOST:-127.0.0.1}"
CPU_LIMIT="${MADMAIL_DOCKER_CPU:-1}"
MEM_LIMIT="${MADMAIL_DOCKER_MEMORY:-1g}"
WITH_ADMIN=0
REBUILD=0
PURGE=0

usage() {
	cat <<EOF
Usage: $(basename "$0") [options]

Run madmail in Docker with state in $DATA_DIR.

Options:
  --with-webadmin   Enable embedded admin UI at /admin
  --rebuild         Force docker image rebuild before deploy
  --purge           Stop container, remove image, delete ./data/docker-vm (make docker-down)
  -h, --help        Show this help

Environment:
  MADMAIL_DOCKER_IMAGE   Image tag (default: madmail-local:docker)
  MADMAIL_DOCKER_NAME    Container name (default: madmail-docker)
  MADMAIL_DOCKER_HOST    Install/listen IP (default: 127.0.0.1)
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

command -v docker >/dev/null || {
	echo "docker is required on PATH" >&2
	exit 1
}

container_running() {
	docker ps --filter "name=^${CONTAINER}$" --filter "status=running" -q | grep -q .
}

if [[ $PURGE -eq 1 ]]; then
	if docker ps -a --filter "name=^${CONTAINER}$" -q | grep -q .; then
		echo "-- Stopping and removing Docker container $CONTAINER ..."
		docker rm -f "$CONTAINER" >/dev/null
	else
		echo "-- Container $CONTAINER does not exist"
	fi
	if docker image inspect "$IMAGE" &>/dev/null; then
		echo "-- Removing Docker image $IMAGE ..."
		docker rmi "$IMAGE" >/dev/null
	fi
	if [[ -d "$DATA_DIR" ]]; then
		echo "-- Deleting host data $DATA_DIR ..."
		rm -rf "$DATA_DIR"
	fi
	echo "-- Docker madmail environment removed"
	exit 0
fi

if [[ $REBUILD -eq 1 ]] || ! docker image inspect "$IMAGE" &>/dev/null; then
	echo "-- Building Docker image $IMAGE ..."
	docker build -t "$IMAGE" "$ROOT"
fi

mkdir -p "$LIB_DIR" "$ETC_DIR" "$RUN_DIR"

if docker ps -a --filter "name=^${CONTAINER}$" -q | grep -q .; then
	echo "-- Removing existing container $CONTAINER ..."
	docker rm -f "$CONTAINER" >/dev/null
fi

CONF="$ETC_DIR/madmail.conf"
if [[ ! -f "$CONF" ]]; then
	echo "-- Bootstrapping with madmail install (--simple --ip $HOST_IP) ..."
	docker run --rm \
		--cap-add NET_BIND_SERVICE \
		-p 80:80 \
		-v "$LIB_DIR:/var/lib/madmail" \
		-v "$ETC_DIR:/etc/madmail" \
		"$IMAGE" \
		install --simple --ip "$HOST_IP" \
			--tls-mode self_signed \
			--enable-chatmail \
			--enable-iroh \
			--skip-systemd --skip-user --non-interactive
fi

echo "-- Starting Docker container $CONTAINER (${CPU_LIMIT} CPU, ${MEM_LIMIT} memory) ..."
docker run -d --name "$CONTAINER" \
	--restart unless-stopped \
	--cpus="$CPU_LIMIT" \
	--memory="$MEM_LIMIT" \
	--cap-add NET_BIND_SERVICE \
	-p 25:25 -p 80:80 -p 443:443 \
	-p 143:143 -p 465:465 -p 587:587 -p 993:993 \
	-p 3478:3478/udp -p 49152-65535:49152-65535/udp \
	-v "$LIB_DIR:/var/lib/madmail" \
	-v "$ETC_DIR:/etc/madmail:ro" \
	-v "$RUN_DIR:/run/madmail" \
	"$IMAGE" >/dev/null

if [[ $WITH_ADMIN -eq 1 ]]; then
	docker exec "$CONTAINER" madmail admin-web path /admin
	docker exec "$CONTAINER" madmail admin-web enable
	docker restart "$CONTAINER" >/dev/null
else
	docker exec "$CONTAINER" madmail admin-web disable 2>/dev/null || true
fi

echo "-- Waiting for madmail to become ready ..."
for _ in $(seq 1 60); do
	if container_running && curl -sk -o /dev/null -w '%{http_code}' "https://${HOST_IP}/" | grep -qE '^[23]'; then
		break
	fi
	sleep 1
done

TOKEN="$(docker exec "$CONTAINER" madmail admin-token --raw 2>/dev/null || true)"

echo ""
echo "== madmail deployed (docker) =="
echo "  container:  $CONTAINER"
echo "  image:      $IMAGE"
echo "  host:       $HOST_IP"
echo "  data:       $DATA_DIR"
echo "  imap:       $HOST_IP:993 (TLS)"
echo "  smtp:       $HOST_IP:465 / $HOST_IP:587 (TLS)"
echo "  web:        https://$HOST_IP/"

if [[ -n "$TOKEN" ]]; then
	echo "  admin-token: $TOKEN"
fi

if [[ $WITH_ADMIN -eq 1 ]]; then
	echo "  admin-ui:   https://$HOST_IP/admin/"
fi

echo ""
echo "Logs:    docker logs -f $CONTAINER"
echo "Shell:   docker exec -it $CONTAINER sh"
echo "Upgrade: $0 --rebuild"
echo "Remove:  make docker-down"