# Building from source

## System dependencies

You need C toolchain, Go toolchain and Make:

On Debian-based system this should work:
```
apt-get install golang-1.23 gcc libc6-dev make
```

Additionally, if you want manual pages, you should also have scdoc installed.
Figuring out the appropriate way to get scdoc is left as an exercise for
reader (for Ubuntu 22.04 LTS it is in repositories).

## Recent Go toolchain

maddy depends on a rather recent Go toolchain version that may not be
available in some distributions (*cough* Debian *cough*).

`go` command in Go 1.21 or newer will automatically download up-to-date
toolchain to build maddy. It is necessary to run commands below only
if you have `go` command version older than 1.21.

```
wget "https://go.dev/dl/go1.23.5.linux-amd64.tar.gz"
tar xf "go1.23.5.linux-amd64.tar.gz"
export GOROOT="$PWD/go"
export PATH="$PWD/go/bin:$PATH"
```

## Step-by-step

1. Clone repository
```
# clone this fork (or the upstream maddy if you prefer)
$ git clone https://github.com/themadorg/madmail.git
$ cd maddy_chatmail
```

2. Select the appropriate version to build or use a pre-built release:

Pre-built release artifacts are published to the GitHub Releases page for this repository. Downloads include platform-specific archives (tar.gz or zip) containing the `maddy` binary and default config files. Visit:

https://github.com/themadorg/madmail/releases

You can also build locally from a tag or branch:
```
$ git checkout v0.8.3      # a specific release (example)
$ git checkout master      # next bugfix release
$ git checkout dev         # next feature release
```

3. Build & install it

Using the provided build script (this follows the original upstream workflow):
```
$ ./build.sh
$ sudo ./build.sh install
```

Using GoReleaser locally (convenient for testing and for producing the same artifacts as CI):

Prerequisites: `goreleaser` installed locally. Then run in snapshot mode (does not publish to GitHub):

```
# build snapshot artifacts locally (no GitHub release)
goreleaser build --snapshot --clean
```

To create an actual GitHub Release with artifacts, push a semver tag like `v0.8.3` — the repository's GitHub Actions workflow will run GoReleaser and publish artifacts automatically.

4. Finish setup as described in [Setting up](../setting-up) (starting from System configuration).


