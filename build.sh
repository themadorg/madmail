#!/bin/sh

destdir=/
builddir="$PWD/build"
prefix=/usr/local
version=
static=0
if [ "${GOFLAGS}" = "" ]; then
	GOFLAGS="-trimpath" # set some flags to avoid passing "" to go
fi

print_help() {
	cat >&2 <<EOF
Usage:
	./build.sh [options] {build,install}

Script to build, package or install Maddy Mail Server.

Options:
    -h, --help              guess!
    --builddir              directory to build in (default: $builddir)

Options for ./build.sh build:
    --static                build static self-contained executables (musl-libc recommended)
    --tags <tags>           build tags to use
    --version <version>     version tag to embed into executables (default: auto-detect)

Additional flags for "go build" can be provided using GOFLAGS environment variable.

Options for ./build.sh install:
    --prefix <path>         installation prefix (default: $prefix)
    --destdir <path>        system root (default: $destdir)
EOF
}

while :; do
	case "$1" in
		-h|--help)
		   print_help
		   exit
		   ;;
		--builddir)
		   shift
		   builddir="$1"
		   ;;
		--prefix)
		   shift
		   prefix="$1"
		   ;;
		--destdir)
			shift
			destdir="$1"
			;;
		--version)
			shift
			version="$1"
			;;
		--static)
			static=1
			;;
		--tags)
			shift
			tags="$1"
			;;
		--)
			break
			shift
			;;
		-?*)
			echo "Unknown option: ${1}. See --help." >&2
			exit 2
			;;
		*)
			break
	esac
	shift
done

configdir="${destdir}etc/maddy"

if [ "$version" = "" ]; then
	version=unknown
	if [ -e .version ]; then
		version="$(cat .version)"
	fi
	if [ -e .git ] && command -v git 2>/dev/null >/dev/null; then
		version="${version}+$(git rev-parse --short HEAD)"
	fi
fi

set -e

build_man_pages() {
	set +e
	if ! command -v scdoc >/dev/null 2>/dev/null; then
		echo '-- [!] No scdoc utility found. Skipping man pages building.' >&2
		set -e
		return
	fi
	set -e

	echo '-- Building man pages...' >&2

	mkdir -p "${builddir}/man"
	for f in ./docs/man/*.1.scd; do
		scdoc < "$f" > "${builddir}/man/$(basename "$f" .scd)"
	done
}

download_iroh_relay() {
	echo "-- Checking for latest iroh-relay release..." >&2
	ASSETS_DIR="internal/endpoint/iroh/assets"
	mkdir -p "$ASSETS_DIR"

	# Use iroh-relay v0.35.0 to match client version
	LATEST_VERSION="v0.35.0"
	# Skip the GitHub API when a synced checkout already has the right binary
	# (avoids failing the whole build when the builder has no API access).
	if [ -f "$ASSETS_DIR/VERSION" ] && [ "$(cat "$ASSETS_DIR/VERSION")" = "$LATEST_VERSION" ] && [ -f "$ASSETS_DIR/iroh-relay" ]; then
		echo "-- iroh-relay is up to date ($LATEST_VERSION)." >&2
		return
	fi

	RELEASE_JSON=$(curl -s https://api.github.com/repos/n0-computer/iroh/releases/tags/${LATEST_VERSION})

	echo "-- Downloading iroh-relay $LATEST_VERSION..." >&2

	ARCH=$(uname -m)
	case "$ARCH" in
		x86_64) IROH_ARCH="x86_64-unknown-linux-musl" ;;
		aarch64|arm64) IROH_ARCH="aarch64-unknown-linux-musl" ;;
		*) echo "-- [!] Unsupported architecture for iroh-relay: $ARCH" >&2; return ;;
	esac

	DOWNLOAD_URL=$(echo "$RELEASE_JSON" | grep "browser_download_url" | grep "iroh-relay-$LATEST_VERSION-$IROH_ARCH.tar.gz" | cut -d '"' -f 4)

	if [ -z "$DOWNLOAD_URL" ]; then
		echo "-- [!] Could not find download URL for iroh-relay $LATEST_VERSION on $ARCH" >&2
		return
	fi

	curl -L "$DOWNLOAD_URL" -o "$ASSETS_DIR/iroh-relay.tar.gz"
	tar -xzf "$ASSETS_DIR/iroh-relay.tar.gz" -C "$ASSETS_DIR"
	
	# Try to find the binary if it's in a subfolder or has a different name
	if [ ! -f "$ASSETS_DIR/iroh-relay" ]; then
		RELAY_BIN=$(find "$ASSETS_DIR" -name iroh-relay -type f | head -n 1)
		if [ -n "$RELAY_BIN" ] && [ "$RELAY_BIN" != "$ASSETS_DIR/iroh-relay" ]; then
			mv "$RELAY_BIN" "$ASSETS_DIR/iroh-relay"
		fi
	fi
	
	rm -f "$ASSETS_DIR/iroh-relay.tar.gz"
	rm -f "$ASSETS_DIR/iroh-relay.tmp"
	echo "$LATEST_VERSION" > "$ASSETS_DIR/VERSION"
	chmod +x "$ASSETS_DIR/iroh-relay"
	echo "-- iroh-relay $LATEST_VERSION downloaded and prepared." >&2
}

copy_admin_web() {
	ADMIN_WEB_SRC="admin-web"
	ADMIN_WEB_BUILD="admin-web/build"
	ADMIN_WEB_DEST="internal/adminweb/build"

	# Build the frontend if package.json exists
	if [ -f "$ADMIN_WEB_SRC/package.json" ]; then
		if command -v bun >/dev/null 2>&1; then
			echo "-- Building admin-web (bun)..." >&2
			(cd "$ADMIN_WEB_SRC" && bun install && bun run build) >&2
		elif command -v npm >/dev/null 2>&1; then
			echo "-- Building admin-web (npm)..." >&2
			(cd "$ADMIN_WEB_SRC" && npm install && npm run build) >&2
		else
			echo "-- [!] No bun or npm found. Skipping admin-web build." >&2
		fi
	fi

	# Stamp version.json with the current build version so the service worker
	# can detect new deployments and invalidate its cache.
	if [ -d "$ADMIN_WEB_BUILD" ]; then
		echo "{\"version\":\"${version}\"}" > "$ADMIN_WEB_BUILD/version.json"
		echo "-- Stamped admin-web version.json with ${version}" >&2
	fi

	# Copy the build output for go:embed
	if [ -d "$ADMIN_WEB_BUILD" ] && [ -f "$ADMIN_WEB_BUILD/index.html" ]; then
		echo "-- Copying admin-web build to $ADMIN_WEB_DEST..." >&2
		rm -rf "$ADMIN_WEB_DEST"
		cp -r "$ADMIN_WEB_BUILD" "$ADMIN_WEB_DEST"
	else
		echo "-- [!] admin-web not built (no $ADMIN_WEB_BUILD/index.html). Admin web UI will not be available." >&2
		# Ensure the placeholder directory exists for go:embed
		mkdir -p "$ADMIN_WEB_DEST"
		if [ ! -f "$ADMIN_WEB_DEST/placeholder" ]; then
			echo "placeholder" > "$ADMIN_WEB_DEST/placeholder"
		fi
	fi
}

build() {
	download_iroh_relay
	copy_admin_web
	mkdir -p "${builddir}"
	echo "-- Version: ${version}" >&2
	if [ "$(go env CC)" = "" ]; then
        echo '-- [!] No C compiler available. maddy will be built without SQLite3 support and default configuration will be unusable.' >&2
    fi

	binary_name="maddy"
	if [ -n "$GOARCH" ]; then
		binary_name="maddy-$GOARCH"
	fi

	if [ "$static" -eq 1 ]; then
		echo "-- Building main server executable ($binary_name)..." >&2
		# This is literally impossible to specify this line of arguments as part of ${GOFLAGS}
		# using only POSIX sh features (and even with Bash extensions I can't figure it out).
		go build -trimpath -buildmode pie -tags "$tags osusergo netgo static_build" \
			-ldflags "-s -w -extldflags '-fno-PIC -static' -X \"github.com/themadorg/madmail/framework/config.Version=${version}\"" \
			-o "${builddir}/${binary_name}" ${GOFLAGS} ./cmd/maddy
	else
		echo "-- Building main server executable ($binary_name)..." >&2
		go build -tags "$tags" -trimpath -ldflags="-s -w -X \"github.com/themadorg/madmail/framework/config.Version=${version}\"" -o "${builddir}/${binary_name}" ${GOFLAGS} ./cmd/maddy
	fi

	build_man_pages

	echo "-- Copying misc files..." >&2

	mkdir -p "${builddir}/systemd"
	cp dist/systemd/*.service "${builddir}/systemd/"
	cp maddy.conf "${builddir}/maddy.conf"
}

install() {
	echo "-- Installing built files..." >&2

	command install -m 0755 -d "${destdir}/${prefix}/bin/"
	command install -m 0755 "${builddir}/maddy" "${destdir}/${prefix}/bin/"
	command ln -sf maddy "${destdir}/${prefix}/bin/maddyctl"
	command install -m 0755 -d "${configdir}"


	# We do not want to overwrite existing configuration.
	# If the file exists, then save it with .default suffix and warn user.
	if [ ! -e "${configdir}/maddy.conf" ]; then
		command install -m 0644 ./maddy.conf "${configdir}/maddy.conf"
	else
		echo "-- [!] Configuration file ${configdir}/maddy.conf exists, saving to ${configdir}/maddy.conf.default" >&2
		command install -m 0644 ./maddy.conf "${configdir}/maddy.conf.default"
	fi

	# Attempt to install systemd units only for Linux.
	# Check is done using GOOS instead of uname -s to account for possible
	# package cross-compilation.
	# Though go command might be unavailable if build.sh is run
	# with sudo and go installation is user-specific, so fallback
	# to using uname -s in the end.
	set +e
	if command -v go >/dev/null 2>/dev/null; then
		set -e
		if [ "$(go env GOOS)" = "linux" ]; then
			command install -m 0755 -d "${destdir}/${prefix}/lib/systemd/system/"
			command install -m 0644 "${builddir}"/systemd/*.service "${destdir}/${prefix}/lib/systemd/system/"
		fi
	else
		set -e
		if [ "$(uname -s)" = "Linux" ]; then
			command install -m 0755 -d "${destdir}/${prefix}/lib/systemd/system/"
			command install -m 0644 "${builddir}"/systemd/*.service "${destdir}/${prefix}/lib/systemd/system/"
		fi
	fi

	if [ -e "${builddir}"/man ]; then
		command install -m 0755 -d "${destdir}/${prefix}/share/man/man1/"
		for f in "${builddir}"/man/*.1; do
			command install -m 0644 "$f" "${destdir}/${prefix}/share/man/man1/"
		done
	fi
}

# Old build.sh compatibility
install_pkg() {
	echo "-- [!] Replace 'install_pkg' with 'install' in build.sh invocation" >&2
	install
}
package() {
	echo "-- [!] Replace 'package' with 'build' in build.sh invocation" >&2
	build
}

if [ $# -eq 0 ]; then
	build
else
	for arg in "$@"; do
		eval "$arg"
	done
fi
