# Madmail themes

Three theme families, each in two dark flavours, selectable as 30 variants across the
public site and the bundled web client.

    primer       the gentle GitHub-inspired one; also the fallback for anything unbuilt
    millennium   blue and chartreuse
    frontier     red on near-black, or red on white

## Setting up

### First, serve from a directory

The themes install into the served tree, so the server has to be serving from one rather than
from its embedded assets. That is the existing customisation route — `madmail extract-www`,
chown to the `madmail` user, `www_dir` in the config, restart — documented on the server's own
**/docs/serve** page. Do that first; `MADMAIL_WWW` below defaults to `/var/lib/madmail/www`,
the folder that guide recommends.

Extract from a build that includes the markup changes described below. From a release without
them the palettes still apply, but everything resting on the hooks is inert.

### The switcher

Put this directory wherever suits you, then put the switcher on your PATH:

    sudo ln -s /path/to/themes/switch /usr/local/bin/madmail-theme

Two paths can be overridden from the environment, both defaulting sensibly:

    MADMAIL_THEMES_DIR   where src/ and compose live. Defaults to the directory
                         holding the switcher, resolved through symlinks.
    MADMAIL_WWW          the served tree. Defaults to /var/lib/madmail/www, which
                         is madmail's default www_dir. Set it if yours differs.

Three optional local hooks, none of them needed for a normal install: a `main.css.<name>`
file beside the switcher installs under that name as-is, a `www.<name>/` directory installs
as a whole served tree, and a `www.themed/` directory, if present, is copied in ahead of
the stylesheets — for an operator keeping a customised markup tree alongside the palettes.

### These themes need the accompanying markup changes

The themes declare custom properties that the site pages and the web client read
through fallback chains. Against **unmodified** upstream markup the palettes still
apply correctly, but everything resting on those hooks is inert: the invite band
colour, the channel and contact selection tints, the status pill treatments, the
start-mode mechanism, the docs table-header fix, and the loading splash dither.

So apply the markup changes first. Without them the themes degrade rather than
break, but they are not complete on their own.

## Installing a theme

    madmail-theme                      # active theme, and the full list
    madmail-theme frontier-dark        # install a variant
    madmail-theme primer-light-soft

To go back to the server's own stylesheets, re-run the `madmail extract-www` step from
**/docs/serve**. A theme only ever overwrites files inside the served tree, so
re-extracting restores the stock appearance exactly, with nothing of a theme left behind.

Every theme has both a light and a dark palette, and both surfaces carry a toggle.
Mode selection is, highest priority first:

1. the visitor's explicit choice, stored in `localStorage` under `madmail-theme`
2. the theme's own `--default-mode` / `--dc-default-mode`
3. the browser's `prefers-color-scheme`

So the operator picks the theme and its opening mode; the visitor can override it in one
click, and that choice persists across the site and the app.

## The App link

Installing a theme reveals a nav item pointing at the bundled web client at `/app.html`,
which is a work in progress and says so on load. That item is hidden in the stock
stylesheets and appears only because each theme here declares
`--nav-app-display: list-item`. To keep it hidden, remove that one declaration from the
theme's `src/*.main.css`.

## Variant naming

    <family>-<site mode>[-soft][-mix[-light]]

- **site mode** — `dark` or `light`, the mode the **site** opens in.
- **`-soft`** — which of the family's two dark palettes the theme uses. A family has one
  light palette and two darks: the standard one, and a softer, lifted one. The word names
  the theme's dark flavour, and it appears **once**.
- **`-mix`** — the **app** departs from the site. What it departs *to* follows from where
  the site started: from a dark site the app takes the other dark flavour; from a light
  site it takes dark.
- **`-mix-light`** — only from a dark site, which is the one case with two possible
  departures: the app opens light.

Ten per family:

| variant | site shows | app shows |
|---|---|---|
| `<f>-dark` | dark | dark |
| `<f>-dark-mix` | dark | the soft dark |
| `<f>-dark-mix-light` | dark | light |
| `<f>-dark-soft` | soft dark | soft dark |
| `<f>-dark-soft-mix` | soft dark | the standard dark |
| `<f>-dark-soft-mix-light` | soft dark | light |
| `<f>-light` | light | light, toggling to the standard dark |
| `<f>-light-soft` | light | light, toggling to the soft dark |
| `<f>-light-mix` | light | dark |
| `<f>-light-soft-mix` | light | the soft dark |

## Structure — three kinds of file

### `src/` — the themes. The only files you edit.

Six flavours, twelve files. A flavour is a family name, optionally `-soft`. There is no
mode in the filename because **each file carries both palette blocks**.

    src/millennium.main.css        src/millennium.app.css
    src/millennium-soft.main.css   src/millennium-soft.app.css
    src/frontier.main.css          src/frontier.app.css
    src/frontier-soft.main.css     src/frontier-soft.app.css
    src/primer.main.css            src/primer.app.css
    src/primer-soft.main.css       src/primer-soft.app.css

In the site files, `:root` is the light palette and `:root.dark` the dark one. In the app
files, `:root` is the dark palette and `.light` the light one — `:root` is used for dark
because it always applies, and the start-up code reads from it before any class is set.

Each flavour's site file feeds five variants' sites, and its app file feeds five variants'
apps. That is the entire blast radius of an edit, and it is readable from the name.

### Shared markup — one copy, identical for every theme

`app.html`, the site and docs pages, and the JavaScript. Theme-specific values appear as
custom properties with fallback chains:

    style="background: var(--dc-invite-bg, var(--dc-surface-2))"

A theme that declares `--dc-invite-bg` gets its own colour; a theme that doesn't falls
through to the original. So adding a hook touches every theme but **changes nothing until
some theme declares the variable**. That property is what keeps the markup shared without
coupling the themes to each other.

### Generated — never stored

The 30 variants are composed at install time by `./compose`, which reads a flavour pair,
writes exactly two declarations — the site's `--default-mode` and the app's
`--dc-default-mode` — rewrites the attribution comment, and installs. **It never reads or
writes a colour.** Nothing is generated to disk and kept, so there are no derived files to
edit by mistake or to drift out of step.

## Adding a theme

1. Copy an existing flavour pair to `src/<name>.main.css` and `src/<name>.app.css`, or
   copy `primer`/`primer-soft` if you want the gentlest starting point.
2. Change the values in the two palette blocks of each. Leave the rules alone unless you
   mean to.
3. Add the family name to `FAMILIES` in both `compose` and the switcher.
4. All ten variants of your family exist immediately.

Only building one mode is fine — take primer's palette block for the mode you are not
theming, so the toggle still leads somewhere reasonable.

## Conventions worth keeping

- **Edit `src/` only.** Anything in `/var/lib/madmail/www` is overwritten at the next
  install, and the switcher will report the theme as `STALE` meanwhile.
- **Use role names, not literals.** `var(--link)` rather than a hex value, so a rule works
  in both modes. A hardcoded colour in an unscoped rule is a bug waiting for someone to
  toggle: scope it `:root.dark .thing { … }` or point it at a variable.
- **Put intensities behind a named knob** so tuning is one value rather than four edits.
  The ones in use on the site: `--alert-fill` (note and alert box fills),
  `--focus-glow-mix`, and `--pill-fill` with `--pill-fg` (the HTTP method pills, the
  relay status pills and the mode badges, which share one strength so they cannot
  disagree with each other). In the app: `--dc-chan-sel-fill` (selected row tints),
  `--dc-brand-border-fill` (the edge on a translucent brand fill),
  `--dc-quiet-border-fill` (the edge on a fill-less button), `--dc-splash-noise` (the
  loading splash's anti-banding dither) and `--dc-splash-bg` (the splash fill, for when
  it should match the site's page background rather than the app's canvas). Each has a
  fallback in its rule, so a theme only declares one when it wants to differ.
- **Two more site hooks sit behind fallbacks.** `--note-link` colours links inside note
  boxes, where the body link colour can be unreadable against a tinted fill, and
  `--code-header-bg` the code-box headers on the docker pages.
- **The mode toggle has two of its own.** `--toggle-knob` is the sliding pip, defaulting to
  `--primary`, and `--toggle-knob-fg` the sun and moon drawn on it, defaulting to white.
  Set the second if your pip is a pale colour, or the icon disappears into it.
- **Header colour is separate from `--muted`.** `--nav-link` paints the nav links and the
  mobile menu toggle, `--nav-accent` the App item. Moving them leaves the footer, form
  labels, doc-list descriptions and `.text-muted` alone, which `--muted` still owns.
- **Small differences between themes are usually deliberate.** Near-identical greys often
  carry intentional hue leans. Don't consolidate them.

## Verifying an install

`madmail-theme` with no argument reports one of:

    verified                      the served files match the sources
    STALE - live files differ     the sources changed, or the served tree was edited
    CANNOT VERIFY - compose ...   the composer failed; the message says why, and whether
                                  the served tree still matches the recorded install

## Attribution and licence

Upstream Madmail stylesheets are Copyright (C) 2026 themadorg, AGPL-3.0-or-later. The
millennium, frontier and primer palettes and their component treatments are by lebavard,
under the same licence.

**primer** follows GitHub's Primer design system closely — its published tokens and, for
most elements, its rules, across all variants.

**frontier**'s light palette took its red and slate greys from the EFF website; nothing
else carries over, and the darks are original.
