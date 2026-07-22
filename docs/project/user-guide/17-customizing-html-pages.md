# Customizing the HTML Pages and Web Interface

Every chatmail server serves a public web interface. This includes the main page, registration (`/new`), contact sharing, invite pages, documentation at `/docs/`, and other assets.

By default it uses the built-in pages that ship with the server. You can customize these pages to match your own branding, language, or requirements.

## Why Customize the HTML?

Common reasons include:

- Adding your own logo, colors, or domain branding.
- Providing instructions in your local language.
- Adding specific rules, legal notices, or contact information.
- Creating a minimal or "stealth" public appearance.
- Hosting your own customized version of the documentation.

## How It Works

The server supports a setting called `www_dir`. When this is configured, the server will look for files in your custom directory first. If a requested file is not found there, it falls back to the built-in version.

This means you only need to provide the files you want to change — everything else continues to work normally.

### Setting a Custom WWW Directory

Add the following to your configuration file:

```toml
www_dir = "/var/lib/maddy/www"
```

Or use the CLI:

```bash
madmail html-serve /var/lib/maddy/www
```

To go back to the built-in (embedded) files:

```bash
madmail html-serve embedded
```

Then reload the server:

```bash
madmail reload
```

Make sure the directory is readable by the user the `madmail` service runs as (usually the `madmail` system user).

## Getting Started with Customization

A typical starting workflow is:

1. Create your `www_dir` (for example `/var/lib/maddy/www`).
2. Copy the default files you want to customize from the server's built-in assets.
3. Edit the copied files.
4. Reload the server and test.

Many operators start by copying the entire default web tree and then gradually replacing individual files as needed.

## What You Can Customize

You can override most public-facing pages and assets, including:

- The main landing page (`index.html`)
- Registration and invite pages
- Contact sharing pages (`/share`)
- Documentation pages under `/docs/`
- CSS, images, and JavaScript
- Error pages

This gives you a lot of flexibility without having to modify the server binary itself.

## The Server's Own Documentation

Every running chatmail server also serves a full set of documentation at:

```
https://your-server/docs/
```

This documentation is multi-language and includes a guide specifically about HTML customization (often reachable at a path like `/docs/custom-html`).

This built-in guide matches the running server version and lists which files you can override and any special features available.

## Security Considerations

- The server will serve any static file it finds in the `www_dir`. Do not place sensitive files (private keys, database files, logs, etc.) in this directory.
- If you are using TLS (strongly recommended), your customized pages will be served over HTTPS.
- Be careful with JavaScript and forms — any custom code runs in the user's browser with the same origin as your server.

## Updating After Changes

In most cases you only need to run:

```bash
madmail reload
```

after changing files in your `www_dir`. A full restart is rarely required for HTML changes.

Users may need to do a hard refresh (Ctrl+Shift+R or Cmd+Shift+R) in their browser to see updated static assets.

## Go Madmail → madmail-v2 (template syntax)

Older Go Madmail pages used Go `html/template` markers such as `{{if .RegistrationOpen}}`, **`{{if not .RegistrationOpen}}`**, and `{{.MailDomain | cleanDomain}}`. madmail-v2 renders with **Minijinja** (`{% if RegistrationOpen %}`, `{% if not RegistrationOpen %}`, `{{ MailDomain | clean_domain }}`).

Leaving Go `{{if not .…}}` unconverted caused template errors (`unexpected '.'`). Runtime conversion and `html-migrate` both handle that form.

If you keep a customized `www_dir` from Go Madmail:

1. After binary upgrade, `madmail update` / `upgrade` offers to convert those files (or run `madmail html-migrate` yourself).
2. Accepting creates `*.go-template.bak` backups and rewrites HTML in place.
3. The server can still convert many Go forms at render time, but migrating on disk keeps your sources consistent with the v2 engine.
4. The same command also fixes **legacy QR** usage (`/qr?data=…` → client-side `setQrCodeImage` + `qrcode.min.js`). The old `/qr` HTTP endpoint is gone.

See the CLI reference: [html-migrate](../../guide/cli/html-migrate.md).

## Literal `{%` and `{{` in custom HTML (Obtainium URLs, etc.)

Minijinja treats **`{%`** and **`{{`** as template syntax everywhere in the file — including inside `href`, scripts, and plain text.

That breaks custom pages that embed deep links or app-store URLs containing those sequences. A common case is **Obtainium**-style links where URL-encoding produces `{%22` (a `{` followed by `%22…`).

**Symptoms:** HTTP 500 on the page; logs like `www template render` / `syntax error` mentioning the file.

**Fix:** wrap the literal content so the engine does not parse it:

```html
{% raw %}
<a href="https://example.com/install?config={%22id%22:%22app%22}">Download</a>
{% endraw %}
```

`html-migrate` **warns** about suspicious `{%` / `{{` sequences but does **not** auto-wrap them (that could break real templates). After editing, reload or restart the service.

## Common Examples

- Community servers adding local rules and support contact information.
- Organizations applying their corporate branding.
- Operators in restricted regions creating a minimal public footprint while keeping full functionality.
- Providing translated documentation tailored to local users.

## Further Reading

The reference shipped with your running server is at `/docs/custom-html` (or the equivalent path). It matches the madmail version you have installed.

You can also explore the `www-src/` directory in the source code to see the default structure and templates.

## Next

- Self-serving binary and built-in docs: see the [Quick Start guide](./02-quick-start.md)
- Managing the server day-to-day: [Admin & CLI](./07-admin-and-cli.md)
