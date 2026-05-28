# External projects (editable)

Git submodules and sibling repos you develop **with** madmailv2. Unlike `context/` (read-only reference trees), this directory is meant for checkouts you edit and commit from.

## Admin panel (SvelteKit)

```bash
git submodule update --init external/madmail-admin-web
cd external/madmail-admin-web && bun install   # or npm install
```

Build and embed into the `chatmail` binary:

```bash
make build-with-admin-web
make restart
```

Override the UI path in `.env`:

```bash
ADMIN_WEB_DIR=external/madmail-admin-web
```

Upstream: [themadorg/madmail-admin-web](https://github.com/themadorg/madmail-admin-web)
