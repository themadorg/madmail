# Madmail themes

Three palette families, each in two flavours, for the public website and the
in-server documentation pages.

    primer       the gentle GitHub-inspired one; also the closest to stock
    millennium   blue and chartreuse
    frontier     red on near-black, or red on white

## Setting up

### First, serve from a directory

Palettes install into the served tree, so the server has to be serving from one
rather than from its embedded assets:

    sudo madmail html-export /var/lib/madmail/www
    sudo chown -R madmail:madmail /var/lib/madmail/www
    sudo madmail html-serve /var/lib/madmail/www
    sudo systemctl restart madmail

`html-serve` writes `www_dir` into `/etc/madmail/madmail.conf`, so the setting
persists. `MADMAIL_WWW` below defaults to `/var/lib/madmail/www`, which is the
path those commands use.

Export from a build that includes the markup changes described below. From a
release without them the palettes still apply, but everything resting on the
hooks is inert.

### The switcher

Put this directory wherever suits you, then put the switcher on your PATH:

    sudo ln -s /path/to/themes/switch /usr/local/bin/madmail-theme

Two paths can be overridden from the environment, both defaulting sensibly:

    MADMAIL_THEMES_DIR   where src/ lives. Defaults to the directory holding
                         the switcher, resolved through symlinks.
    MADMAIL_WWW          the served tree. Defaults to /var/lib/madmail/www.

The switcher writes into the served tree, so run it as an administrator.

### These palettes need the accompanying markup changes

The palettes declare custom properties that the site and documentation pages
read through fallback chains. Against **unmodified** upstream markup the
palettes still apply correctly, but everything resting on those hooks is inert:
the status pill treatments, the docs table-header fix, the note-box link
colour, and the code-box headers on the Docker pages.

So apply the markup changes first. Without them the palettes degrade rather
than break, but they are not complete on their own.

## Installing a palette

    madmail-theme                   # active palette, and the full list
    madmail-theme frontier
    madmail-theme primer-soft

The switcher only ever writes `main.css` inside the served tree, so restoring
the stock appearance is a matter of putting the exported `main.css` back.

Every palette carries both a light and a dark block, and the site's control
cycles between automatic, dark and light. Automatic is the default and follows
the browser's `prefers-color-scheme`, including while the page is open.

### Pinning a default mode

An operator who wants a particular mode on first load, rather than whatever the
visitor's browser asks for, can say so at install time:

    madmail-theme millennium --default-mode dark

This is off unless asked for. It sets the opening mode only — a visitor can
still choose, and a visitor who chooses automatic outranks it. Re-installing
without the flag clears it.

## The palettes

Six files, one per palette. The file name is the palette name, so there is
nothing to register anywhere.

    src/millennium.main.css        src/millennium-soft.main.css
    src/frontier.main.css          src/frontier-soft.main.css
    src/primer.main.css            src/primer-soft.main.css

There is no mode in the filename because **each file carries both palette
blocks**: `:root` is the light palette and `:root.dark` the dark one.

A family has one light palette and two darks — the standard one, and a softer,
lifted one. `-soft` names the second. So `millennium` and `millennium-soft`
are identical in light mode and differ only in the dark block; a visitor whose
browser prefers light sees no difference between them.

### Shared markup — one copy, identical for every palette

The site and documentation pages. Palette-specific values appear as custom
properties with fallback chains:

    background: color-mix(in srgb, var(--success) var(--pill-fill, 15%), transparent)

A palette that declares `--pill-fill` gets its own strength; one that doesn't
falls through to the original. So adding a hook touches every palette but
**changes nothing until some palette declares the variable**. That property is
what keeps the markup shared without coupling the palettes to each other.

Palettes replace `main.css` wholesale rather than layering over it, so any rule
the site needs has to exist in every palette as well as in the stock
stylesheet.

## Adding a palette

1. Copy an existing file to `src/<name>.main.css`, or copy `primer` if you want
   the gentlest starting point.
2. Change the values in the two palette blocks. Leave the rules alone unless you
   mean to.
3. That is all. `madmail-theme <name>` installs it.

Only building one mode is fine — take primer's palette block for the mode you
are not theming, so the control still leads somewhere reasonable.

A palette matching Madmail's own purple would be a natural addition, and on
this layout it is one file.

## Conventions worth keeping

- **Edit `src/` only.** Anything in `/var/lib/madmail/www` is overwritten at the
  next install, and the switcher will report the palette as `STALE` meanwhile.
- **Use role names, not literals.** `var(--link)` rather than a hex value, so a
  rule works in both modes. A hardcoded colour in an unscoped rule is a bug
  waiting for someone to switch mode: scope it `:root.dark .thing { … }` or
  point it at a variable.
- **Put intensities behind a named knob** so tuning is one value rather than
  four edits. The ones in use: `--alert-fill` (note and alert box fills),
  `--focus-glow-mix`, and `--pill-fill` with `--pill-fg` (the HTTP method
  pills, the relay status pills and the mode badges).
- **`--pill-fg` is an absolute colour, `--pill-fill` a percentage.** The
  percentage preserves each pill's own hue; the colour does not. So `--pill-fg`
  belongs only on pills whose hue is too light to carry its own label — in
  millennium, the chartreuse ones. A pill that reads well in its own colour
  keeps it.
- **The colour-mode control takes no hover fill by default.** The site's
  secondary buttons shift their border and text rather than filling, and the
  control follows that. `--mode-hover-bg` exists for a palette that wants a
  fill instead, and defaults to `transparent`, so declaring nothing keeps the
  site's own convention.
- **Two more site hooks sit behind fallbacks.** `--note-link` colours links
  inside note boxes, where the body link colour can be unreadable against a
  tinted fill, and `--code-header-bg` the code-box headers on the Docker pages.
- **Header colour is separate from `--muted`.** `--nav-link` paints the nav
  links and the mobile menu toggle. Moving it leaves the footer, form labels,
  doc-list descriptions and `.text-muted` alone, which `--muted` still owns.
- **Small differences between palettes are usually deliberate.** Near-identical
  greys often carry intentional hue leans. Don't consolidate them.

## Verifying an install

`madmail-theme` with no argument reports one of:

    verified                                      the served main.css matches the source
    STALE - main.css differs from src/<name>…     the source changed, or the served file was edited
    STALE - palette matches, default-mode is …    only the pinned mode differs from what was recorded
    STALE - no main.css in <dir>                  nothing installed there
    unknown palette                               the recorded name has no file in src/

Verification regenerates what an install would write and compares it, rather
than checking a stored fingerprint, so it notices a change in `src/` as well as
an edit to the served tree.

## Attribution and licence

Upstream Madmail stylesheets are Copyright (C) 2026 themadorg,
AGPL-3.0-or-later. The millennium, frontier and primer palettes and their
component treatments are by lebavard, under the same licence.

**primer** follows GitHub's Primer design system closely — its published tokens
and, for most elements, its rules.

**frontier**'s light palette took its red and slate greys from the EFF website;
nothing else carries over, and the darks are original.
